// SPDX-License-Identifier: BUSL-1.1

//! Schema-diff generation (Pro): captures a structured snapshot of a live schema
//! and turns the delta against a baseline into a versioned migration (up + down),
//! reusing the DDL builders. Lossy operations (dropped columns/tables) can recreate
//! the structure but not the data, so their `down` is emitted commented-out.

import { Driver } from '@/lib/connection/drivers';
import {
  type AlterOp,
  buildAlterTableSQL,
  buildCreateTableStatements,
  buildDropTableSQL,
  type ColumnDef,
  type DdlWarning,
  diffTableDefinitions,
  type ForeignKeyDef,
  type IndexDef,
  type NamespaceLike,
  type TableDefinition,
} from '@/lib/ddl';
import {
  type Collection,
  describeTable,
  listCollections,
  listNamespaces,
  type Namespace,
  type TableSchema,
} from '@/lib/tauri';

export interface SchemaSnapshot {
  capturedAt: string;
  driver: Driver;
  /** Database the snapshot is scoped to (null = whole connection). */
  database: string | null;
  /** Tables keyed by schema-qualified name (`db.schema.table`). */
  tables: Record<string, TableDefinition>;
}

export interface CaptureResult {
  snapshot: SchemaSnapshot;
  /** Qualified names of tables that could not be described (surfaced, never silently dropped). */
  failedTables: string[];
}

/** Bounded-concurrency map, so capturing hundreds of tables neither serializes nor floods the driver. */
const DESCRIBE_CONCURRENCY = 8;

async function mapPool<T, R>(
  items: T[],
  limit: number,
  fn: (item: T, index: number) => Promise<R>
): Promise<R[]> {
  const results: R[] = new Array(items.length);
  let cursor = 0;
  const workers = Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (cursor < items.length) {
      const i = cursor++;
      results[i] = await fn(items[i], i);
    }
  });
  await Promise.all(workers);
  return results;
}

/** Enumerates every table of a namespace, paginating so large schemas aren't truncated. */
async function listAllTables(sessionId: string, ns: Namespace): Promise<Collection[]> {
  const pageSize = 500;
  const all: Collection[] = [];
  let page = 1;
  // Hard guard against a driver that never signals the end of pagination.
  for (let guard = 0; guard < 1000; guard++) {
    const res = await listCollections(sessionId, ns, undefined, page, pageSize);
    const batch = res.data?.collections ?? [];
    all.push(...batch);
    const total = res.data?.total_count;
    if (batch.length < pageSize || (typeof total === 'number' && all.length >= total)) break;
    page++;
  }
  return all.filter(c => c.collection_type === 'Table');
}

export interface GeneratedMigration {
  up: string;
  down: string;
  warnings: DdlWarning[];
  /** True when the `down` contains commented-out, data-lossy statements. */
  hasIrreversible: boolean;
  /** True when the schemas are genuinely identical — no operations at all. */
  isEmpty: boolean;
  /**
   * Operations the driver's dialect cannot express (e.g. SQLite column type
   * changes). Non-empty means the schemas DO differ but the generated `up`
   * doesn't say so — the caller must refuse rather than report "no changes".
   */
  unexpressed: AlterOp[];
}

// The DDL builders only switch on the 5 canonical dialects; Postgres-compatible
// drivers must be normalized before generation.
function ddlDriver(driver: Driver): Driver {
  switch (driver) {
    case Driver.Supabase:
    case Driver.Neon:
    case Driver.Timescaledb:
      return Driver.Postgres;
    case Driver.Motherduck:
      return Driver.Duckdb;
    default:
      return driver;
  }
}

function tableKey(ns: NamespaceLike, table: string): string {
  return `${ns.database}.${ns.schema ?? ''}.${table}`;
}

function joinStatements(stmts: string[]): string {
  return stmts
    .map(s => s.trim())
    .filter(Boolean)
    .map(s => (s.endsWith(';') ? s : `${s};`))
    .join('\n');
}

function commentOut(sql: string): string {
  return sql
    .split('\n')
    .map(line => `-- ${line}`)
    .join('\n');
}

export function schemaToDefinition(
  schema: TableSchema,
  namespace: NamespaceLike,
  tableName: string
): TableDefinition {
  const columns: ColumnDef[] = schema.columns.map(col => ({
    name: col.name,
    type: col.data_type,
    nullable: col.nullable,
    defaultValue: col.default_value,
    isPrimaryKey: col.is_primary_key,
    isUnique: false,
    isAutoIncrement: col.is_auto_increment,
  }));

  const foreignKeys: ForeignKeyDef[] = schema.foreign_keys
    .filter(fk => !fk.is_virtual)
    .map(fk => ({
      name: fk.constraint_name,
      columns: [fk.column],
      refSchema: fk.referenced_schema ?? null,
      refTable: fk.referenced_table,
      refColumns: [fk.referenced_column],
    }));

  const indexes: IndexDef[] = schema.indexes
    .filter(idx => !idx.is_primary)
    .map(idx => ({
      name: idx.name,
      columns: idx.columns,
      unique: idx.is_unique,
      method: idx.index_type ?? undefined,
    }));

  return {
    namespace,
    tableName,
    columns,
    foreignKeys,
    indexes,
    primaryKeyName: schema.indexes.find(idx => idx.is_primary)?.name,
  };
}

/**
 * Captures every table of the active database into a structured snapshot.
 * When `database` is given, only that database's namespaces are scanned.
 * Tables that fail to describe are reported in `failedTables` rather than
 * dropped — a missing table would otherwise read as a deletion downstream.
 */
export async function captureSnapshot(
  sessionId: string,
  driver: Driver,
  database?: string
): Promise<CaptureResult> {
  const nsRes = await listNamespaces(sessionId);
  const namespaces: Namespace[] = (nsRes.namespaces ?? []).filter(
    ns => !database || ns.database === database
  );

  const tables: Record<string, TableDefinition> = {};
  const failedTables: string[] = [];

  for (const ns of namespaces) {
    const collections = await listAllTables(sessionId, ns);
    await mapPool(collections, DESCRIBE_CONCURRENCY, async collection => {
      const key = tableKey(ns, collection.name);
      try {
        const res = await describeTable(sessionId, ns, collection.name);
        if (res.success && res.schema) {
          tables[key] = schemaToDefinition(res.schema, ns, collection.name);
        } else {
          failedTables.push(key);
        }
      } catch {
        failedTables.push(key);
      }
    });
  }

  return {
    snapshot: { capturedAt: new Date().toISOString(), driver, database: database ?? null, tables },
    failedTables,
  };
}

/**
 * Splits the reverse diff into structurally-reversible ops and data-lossy ones
 * (re-adding a dropped column recreates the column but not its data).
 */
export function invertOps(
  before: TableDefinition,
  after: TableDefinition
): { reversible: AlterOp[]; irreversible: AlterOp[] } {
  const reversible: AlterOp[] = [];
  const irreversible: AlterOp[] = [];
  for (const op of diffTableDefinitions(after, before)) {
    if (op.kind === 'add_column') irreversible.push(op);
    else reversible.push(op);
  }
  return { reversible, irreversible };
}

/** Diffs a baseline against a live snapshot and produces up/down migration scripts. */
export function generateMigration(
  before: SchemaSnapshot,
  after: SchemaSnapshot,
  driver: Driver
): GeneratedMigration {
  const d = ddlDriver(driver);
  const up: string[] = [];
  const downReversible: string[] = [];
  const downIrreversible: string[] = [];
  const warnings: DdlWarning[] = [];
  const unexpressed: AlterOp[] = [];
  let anyOps = false;

  const keys = Array.from(
    new Set([...Object.keys(before.tables), ...Object.keys(after.tables)])
  ).sort();

  // Added tables: create in up, drop in down (dropping a freshly-created table is safe).
  for (const key of keys) {
    const b = before.tables[key];
    const a = after.tables[key];
    if (b || !a) continue;
    anyOps = true;
    const res = buildCreateTableStatements(a, d);
    up.push(joinStatements(res.statements));
    warnings.push(...res.warnings);
    downReversible.push(buildDropTableSQL(a.namespace, a.tableName, d));
  }

  // Modified tables: ALTER both ways.
  for (const key of keys) {
    const b = before.tables[key];
    const a = after.tables[key];
    if (!b || !a) continue;
    const ops = diffTableDefinitions(b, a);
    if (ops.length === 0) continue;
    anyOps = true;
    const upSql = buildAlterTableSQL(a, ops, d);
    up.push(joinStatements(upSql.statements));
    warnings.push(...upSql.warnings);
    // Check each operation on its own: a table can mix expressible and
    // inexpressible changes (SQLite ADD COLUMN + type change), and emitting only
    // the half that compiles would silently ship an incomplete migration.
    for (const op of ops) {
      if (buildAlterTableSQL(a, [op], d).statements.length === 0) unexpressed.push(op);
    }

    const { reversible, irreversible } = invertOps(b, a);
    if (reversible.length > 0) {
      const dn = buildAlterTableSQL(b, reversible, d);
      downReversible.push(joinStatements(dn.statements));
      warnings.push(...dn.warnings);
    }
    if (irreversible.length > 0) {
      const dn = buildAlterTableSQL(b, irreversible, d);
      downIrreversible.push(joinStatements(dn.statements));
      warnings.push(...dn.warnings);
    }
  }

  // Dropped tables: drop in up, recreate in down but flagged irreversible (data is gone).
  for (const key of keys) {
    const b = before.tables[key];
    const a = after.tables[key];
    if (!b || a) continue;
    anyOps = true;
    up.push(buildDropTableSQL(b.namespace, b.tableName, d));
    const res = buildCreateTableStatements(b, d);
    downIrreversible.push(joinStatements(res.statements));
    warnings.push(...res.warnings);
  }

  const hasIrreversible = downIrreversible.length > 0;
  const downBlocks = [...downReversible];
  if (hasIrreversible) {
    downBlocks.push(
      [
        '-- The statements below are IRREVERSIBLE (data loss) and are commented out.',
        '-- Review and complete the rollback manually before applying.',
        ...downIrreversible.map(commentOut),
      ].join('\n')
    );
  }

  const upScript = up.join('\n\n').trim();
  return {
    up: upScript,
    down: downBlocks.join('\n\n').trim(),
    warnings,
    hasIrreversible,
    // Empty means "the schemas match", not merely "we produced no SQL".
    isEmpty: !anyOps,
    unexpressed,
  };
}
