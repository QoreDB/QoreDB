// SPDX-License-Identifier: Apache-2.0

import { Driver } from '../drivers';
import { getDdlCapabilities, quoteSqlString } from './driverCapabilities';
import { buildQualifiedTableName, quoteIdentifier } from './identifiers';
import type {
  CheckConstraintDef,
  ColumnDef,
  ForeignKeyDef,
  IndexDef,
  NamespaceLike,
  TableDefinition,
} from './types';

export interface BuildResult {
  statements: string[];
  warnings: string[];
}

interface ColumnSqlOptions {
  suppressInlinePk?: boolean;
  inlineComment?: boolean;
}

export function buildColumnSQL(
  col: ColumnDef,
  driver: Driver,
  opts: ColumnSqlOptions = {}
): string {
  const ident = quoteIdentifier(col.name, driver);
  let sql = `${ident} ${col.type}`;

  if (col.length) {
    sql = `${ident} ${col.type}(${col.length})`;
  } else if (col.precision) {
    sql = `${ident} ${col.type}(${col.precision}${col.scale ? `, ${col.scale}` : ''})`;
  }

  if (!col.nullable) {
    sql += ' NOT NULL';
  }
  if (col.defaultValue !== undefined && col.defaultValue !== '') {
    sql += ` DEFAULT ${col.defaultValue}`;
  }
  if (col.isPrimaryKey && !opts.suppressInlinePk) {
    sql += ' PRIMARY KEY';
  }
  if (col.isUnique && !col.isPrimaryKey) {
    sql += ' UNIQUE';
  }
  if (col.isAutoIncrement && (driver === Driver.Mysql || driver === Driver.Mariadb)) {
    sql += ' AUTO_INCREMENT';
  }
  if (opts.inlineComment && col.comment) {
    sql += ` COMMENT ${quoteSqlString(col.comment)}`;
  }

  return sql;
}

function buildForeignKeyClause(fk: ForeignKeyDef, table: TableDefinition, driver: Driver): string {
  const cols = fk.columns.map(c => quoteIdentifier(c, driver)).join(', ');
  const refCols = fk.refColumns.map(c => quoteIdentifier(c, driver)).join(', ');
  const refQualified = buildQualifiedTableName(
    { database: table.namespace.database, schema: fk.refSchema ?? table.namespace.schema },
    fk.refTable,
    driver
  );

  let clause = '';
  if (fk.name) {
    clause += `CONSTRAINT ${quoteIdentifier(fk.name, driver)} `;
  }
  clause += `FOREIGN KEY (${cols}) REFERENCES ${refQualified} (${refCols})`;
  if (fk.onDelete) clause += ` ON DELETE ${fk.onDelete}`;
  if (fk.onUpdate) clause += ` ON UPDATE ${fk.onUpdate}`;
  return clause;
}

function buildCheckClause(check: CheckConstraintDef, driver: Driver): string {
  let clause = '';
  if (check.name) {
    clause += `CONSTRAINT ${quoteIdentifier(check.name, driver)} `;
  }
  clause += `CHECK (${check.expression})`;
  return clause;
}

function buildCreateIndexSql(
  idx: IndexDef,
  fullName: string,
  driver: Driver,
  warnings: string[]
): string | null {
  const caps = getDdlCapabilities(driver);
  if (!caps.supportsIndexes) {
    warnings.push(`Indexes are not supported on this driver: ${idx.name}`);
    return null;
  }
  if (idx.unique && !caps.supportsUniqueIndex) {
    warnings.push(`UNIQUE index not supported on this driver, emitted as plain index: ${idx.name}`);
  }

  const cols = idx.columns.map(c => quoteIdentifier(c, driver)).join(', ');
  const unique = idx.unique && caps.supportsUniqueIndex ? 'UNIQUE ' : '';
  const indexName = quoteIdentifier(idx.name, driver);

  let stmt = `CREATE ${unique}INDEX ${indexName} ON ${fullName}`;

  if (idx.method) {
    if (!caps.supportsIndexMethod) {
      warnings.push(`Index method "${idx.method}" ignored on this driver: ${idx.name}`);
    } else if (caps.indexMethodPlacement === 'before-columns') {
      stmt += ` USING ${idx.method}`;
    }
  }

  stmt += ` (${cols})`;

  if (idx.method && caps.supportsIndexMethod && caps.indexMethodPlacement === 'after-columns') {
    stmt += ` USING ${idx.method}`;
  }

  if (idx.where) {
    if (caps.supportsPartialIndex) {
      stmt += ` WHERE ${idx.where}`;
    } else {
      warnings.push(`Partial index WHERE clause not supported on this driver: ${idx.name}`);
    }
  }

  return `${stmt};`;
}

export function buildCreateTableStatements(table: TableDefinition, driver: Driver): BuildResult {
  const caps = getDdlCapabilities(driver);
  const warnings: string[] = [];
  const fullName = buildQualifiedTableName(table.namespace, table.tableName, driver);

  const pkCols = table.columns.filter(c => c.isPrimaryKey);
  const useTableLevelPk = pkCols.length > 1;

  const lines: string[] = [];

  for (const col of table.columns) {
    const line = buildColumnSQL(col, driver, {
      suppressInlinePk: useTableLevelPk,
      inlineComment: caps.inlineColumnComments,
    });
    lines.push(line);
    if (col.comment && !caps.inlineColumnComments && !caps.separateColumnComments) {
      warnings.push(`Column comments not supported on this driver: ${col.name}`);
    }
  }

  if (useTableLevelPk) {
    const pkList = pkCols.map(c => quoteIdentifier(c.name, driver)).join(', ');
    lines.push(`PRIMARY KEY (${pkList})`);
  }

  for (const fk of table.foreignKeys ?? []) {
    if (!caps.supportsForeignKeys) {
      warnings.push(
        `Foreign keys not supported on this driver: ${fk.name ?? fk.columns.join(',')}`
      );
      continue;
    }
    lines.push(buildForeignKeyClause(fk, table, driver));
  }

  for (const check of table.checks ?? []) {
    if (!caps.supportsCheckConstraints) {
      warnings.push(`CHECK constraints not supported on this driver: ${check.name ?? '(unnamed)'}`);
      continue;
    }
    lines.push(buildCheckClause(check, driver));
  }

  let createStmt = `CREATE TABLE ${fullName} (\n  ${lines.join(',\n  ')}\n)`;
  if (table.comment && caps.inlineTableComment) {
    createStmt += ` COMMENT=${quoteSqlString(table.comment)}`;
  }
  createStmt += ';';

  const statements: string[] = [createStmt];

  if (table.comment && !caps.inlineTableComment) {
    if (caps.separateTableComment) {
      statements.push(`COMMENT ON TABLE ${fullName} IS ${quoteSqlString(table.comment)};`);
    } else {
      warnings.push('Table comments not supported on this driver');
    }
  }

  if (caps.separateColumnComments && !caps.inlineColumnComments) {
    for (const col of table.columns) {
      if (!col.comment) continue;
      const colIdent = quoteIdentifier(col.name, driver);
      statements.push(
        `COMMENT ON COLUMN ${fullName}.${colIdent} IS ${quoteSqlString(col.comment)};`
      );
    }
  }

  for (const idx of table.indexes ?? []) {
    const stmt = buildCreateIndexSql(idx, fullName, driver, warnings);
    if (stmt) statements.push(stmt);
  }

  return { statements, warnings };
}

export function buildCreateTableSQL(
  namespace: NamespaceLike,
  tableName: string,
  columns: ColumnDef[],
  driver: Driver
): string {
  const result = buildCreateTableStatements({ namespace, tableName, columns }, driver);
  return result.statements.join('\n\n');
}
