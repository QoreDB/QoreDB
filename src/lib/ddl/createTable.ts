// SPDX-License-Identifier: Apache-2.0

import { Driver } from '../drivers';
import { buildQualifiedTableName, quoteIdentifier } from './identifiers';
import type { ColumnDef, NamespaceLike } from './types';

export function buildColumnSQL(col: ColumnDef, driver: Driver): string {
  let sql = `${quoteIdentifier(col.name, driver)} ${col.type}`;

  if (col.length) {
    sql = `${quoteIdentifier(col.name, driver)} ${col.type}(${col.length})`;
  } else if (col.precision) {
    sql = `${quoteIdentifier(col.name, driver)} ${col.type}(${col.precision}${col.scale ? `, ${col.scale}` : ''})`;
  }

  if (!col.nullable) {
    sql += ' NOT NULL';
  }
  if (col.defaultValue !== undefined && col.defaultValue !== '') {
    sql += ` DEFAULT ${col.defaultValue}`;
  }
  if (col.isPrimaryKey) {
    sql += ' PRIMARY KEY';
  }
  if (col.isUnique && !col.isPrimaryKey) {
    sql += ' UNIQUE';
  }
  if (col.isAutoIncrement && driver === Driver.Mysql) {
    sql += ' AUTO_INCREMENT';
  }

  return sql;
}

export function buildCreateTableSQL(
  namespace: NamespaceLike,
  tableName: string,
  columns: ColumnDef[],
  driver: Driver
): string {
  const fullName = buildQualifiedTableName(namespace, tableName, driver);
  const columnDefs = columns.map(col => buildColumnSQL(col, driver));
  return `CREATE TABLE ${fullName} (\n  ${columnDefs.join(',\n  ')}\n);`;
}
