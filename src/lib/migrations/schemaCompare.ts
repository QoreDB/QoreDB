// SPDX-License-Identifier: BUSL-1.1

//! Structural comparison between two schema snapshots. Powers both drift
//! detection (baseline vs live, same connection) and the Prod↔Staging schema
//! diff (two connections, matching tables by schema-qualified name). Column-level
//! changes are derived from the single source of truth `diffTableDefinitions`.

import {
  type AlterOp,
  diffTableDefinitions,
  primaryKeyColumns,
  type TableDefinition,
} from '@/lib/ddl';
import type { SchemaSnapshot } from './schemaDiff';

export type TableStatus = 'added' | 'removed' | 'modified';

export interface ColumnChange {
  kind:
    | 'added'
    | 'removed'
    | 'type_changed'
    | 'nullability_changed'
    | 'default_changed'
    | 'renamed'
    | 'comment_changed';
  column: string;
  detail?: string;
}

export interface ObjectChange {
  kind: 'added' | 'removed';
  name: string;
}

export interface TableChange {
  /** Match key: schema-qualified table name (database-agnostic when comparing connections). */
  key: string;
  table: string;
  schema?: string | null;
  status: TableStatus;
  columns: ColumnChange[];
  indexes: ObjectChange[];
  foreignKeys: ObjectChange[];
  primaryKey: ObjectChange[];
  /** Raw left→right delta, for callers that want to generate reconciling SQL. */
  ops: AlterOp[];
}

export interface SchemaDelta {
  changes: TableChange[];
  hasChanges: boolean;
  summary: { added: number; removed: number; modified: number };
}

export interface CompareOptions {
  /** Match tables by schema.table only, ignoring the database (for cross-connection diffs). */
  ignoreDatabase?: boolean;
  /**
   * Keys (in the compared keyspace) to exclude entirely — e.g. tables that failed
   * to describe, so a transient capture failure isn't reported as a deletion.
   */
  ignoreKeys?: Set<string>;
}

/** Strips the leading `db.` segment from a `db.schema.table` key to `schema.table`. */
export function stripDatabaseKey(key: string): string {
  const i = key.indexOf('.');
  return i === -1 ? key : key.slice(i + 1);
}

function reKey(snapshot: SchemaSnapshot, ignoreDatabase: boolean): Map<string, TableDefinition> {
  const map = new Map<string, TableDefinition>();
  for (const [key, def] of Object.entries(snapshot.tables)) {
    if (ignoreDatabase) {
      map.set(`${def.namespace.schema ?? ''}.${def.tableName}`, def);
    } else {
      map.set(key, def);
    }
  }
  return map;
}

function nullability(nullable: boolean): string {
  return nullable ? 'NULL' : 'NOT NULL';
}

function opsToChanges(
  ops: AlterOp[],
  before: TableDefinition
): {
  columns: ColumnChange[];
  indexes: ObjectChange[];
  foreignKeys: ObjectChange[];
  primaryKey: ObjectChange[];
} {
  const columns: ColumnChange[] = [];
  const indexes: ObjectChange[] = [];
  const foreignKeys: ObjectChange[] = [];
  const primaryKey: ObjectChange[] = [];
  const beforeCol = (name: string) => before.columns.find(c => c.name === name);

  for (const op of ops) {
    switch (op.kind) {
      case 'add_column':
        columns.push({ kind: 'added', column: op.column.name, detail: op.column.type });
        break;
      case 'drop_column':
        columns.push({ kind: 'removed', column: op.columnName });
        break;
      case 'change_type':
        columns.push({
          kind: 'type_changed',
          column: op.columnName,
          detail: `${beforeCol(op.columnName)?.type ?? '?'} → ${op.newType}`,
        });
        break;
      case 'set_nullable':
        columns.push({
          kind: 'nullability_changed',
          column: op.columnName,
          detail: `${nullability(beforeCol(op.columnName)?.nullable ?? true)} → ${nullability(op.nullable)}`,
        });
        break;
      case 'set_default':
        columns.push({
          kind: 'default_changed',
          column: op.columnName,
          detail: `${beforeCol(op.columnName)?.defaultValue ?? '∅'} → ${op.defaultValue ?? '∅'}`,
        });
        break;
      case 'set_column_comment':
        columns.push({ kind: 'comment_changed', column: op.columnName });
        break;
      case 'rename_column':
        columns.push({ kind: 'renamed', column: `${op.from} → ${op.to}` });
        break;
      case 'add_index':
        indexes.push({ kind: 'added', name: op.index.name });
        break;
      case 'drop_index':
        indexes.push({ kind: 'removed', name: op.name });
        break;
      case 'add_foreign_key':
        foreignKeys.push({
          kind: 'added',
          name: op.foreignKey.name ?? op.foreignKey.columns.join(','),
        });
        break;
      case 'drop_foreign_key':
        foreignKeys.push({ kind: 'removed', name: op.name });
        break;
      case 'add_primary_key':
        primaryKey.push({ kind: 'added', name: op.columns.join(', ') });
        break;
      case 'drop_primary_key':
        primaryKey.push({ kind: 'removed', name: op.name ?? primaryKeyColumns(before).join(', ') });
        break;
      // Table-level and check ops carry no per-column drift signal in v1.
      default:
        break;
    }
  }
  return { columns, indexes, foreignKeys, primaryKey };
}

/** Compares two snapshots into a structured, display-ready delta (left = reference, right = current). */
export function compareSnapshots(
  left: SchemaSnapshot,
  right: SchemaSnapshot,
  opts: CompareOptions = {}
): SchemaDelta {
  const ignoreDatabase = opts.ignoreDatabase ?? false;
  const ignoreKeys = opts.ignoreKeys;
  const leftMap = reKey(left, ignoreDatabase);
  const rightMap = reKey(right, ignoreDatabase);
  const keys = Array.from(new Set([...leftMap.keys(), ...rightMap.keys()])).sort();

  const changes: TableChange[] = [];
  for (const key of keys) {
    if (ignoreKeys?.has(key)) continue;
    const l = leftMap.get(key);
    const r = rightMap.get(key);

    if (l && !r) {
      changes.push({
        key,
        table: l.tableName,
        schema: l.namespace.schema,
        status: 'removed',
        columns: [],
        indexes: [],
        foreignKeys: [],
        primaryKey: [],
        ops: [],
      });
      continue;
    }
    if (!l && r) {
      changes.push({
        key,
        table: r.tableName,
        schema: r.namespace.schema,
        status: 'added',
        columns: [],
        indexes: [],
        foreignKeys: [],
        primaryKey: [],
        ops: [],
      });
      continue;
    }
    if (l && r) {
      const ops = diffTableDefinitions(l, r);
      if (ops.length === 0) continue;
      const { columns, indexes, foreignKeys, primaryKey } = opsToChanges(ops, l);
      changes.push({
        key,
        table: r.tableName,
        schema: r.namespace.schema,
        status: 'modified',
        columns,
        indexes,
        foreignKeys,
        primaryKey,
        ops,
      });
    }
  }

  const summary = {
    added: changes.filter(c => c.status === 'added').length,
    removed: changes.filter(c => c.status === 'removed').length,
    modified: changes.filter(c => c.status === 'modified').length,
  };
  return { changes, hasChanges: changes.length > 0, summary };
}
