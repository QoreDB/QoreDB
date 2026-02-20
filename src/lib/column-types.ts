// SPDX-License-Identifier: Apache-2.0

/**
 * Column types and DDL utilities for QoreDB
 *
 * Provides driver-aware column type definitions for table creation
 */

import { Driver, getDriverMetadata } from './drivers';

export type ColumnCategory =
  | 'integer'
  | 'float'
  | 'string'
  | 'text'
  | 'date'
  | 'binary'
  | 'json'
  | 'boolean'
  | 'other';

export interface ColumnType {
  name: string;
  category: ColumnCategory;
  hasLength?: boolean; // VARCHAR(n)
  hasPrecision?: boolean; // DECIMAL(p, s)
  isAutoIncrement?: boolean;
}

export interface ColumnDef {
  name: string;
  type: string;
  length?: number;
  precision?: number;
  scale?: number;
  nullable: boolean;
  defaultValue?: string;
  isPrimaryKey: boolean;
  isUnique: boolean;
  isAutoIncrement?: boolean;
}

export interface NamespaceLike {
  database: string;
  schema?: string | null;
}

// PostgreSQL column types
const POSTGRES_TYPES: ColumnType[] = [
  // Integers
  { name: 'SERIAL', category: 'integer', isAutoIncrement: true },
  { name: 'BIGSERIAL', category: 'integer', isAutoIncrement: true },
  { name: 'SMALLINT', category: 'integer' },
  { name: 'INTEGER', category: 'integer' },
  { name: 'BIGINT', category: 'integer' },
  // Floats
  { name: 'REAL', category: 'float' },
  { name: 'DOUBLE PRECISION', category: 'float' },
  { name: 'NUMERIC', category: 'float', hasPrecision: true },
  { name: 'DECIMAL', category: 'float', hasPrecision: true },
  // Strings
  { name: 'VARCHAR', category: 'string', hasLength: true },
  { name: 'CHAR', category: 'string', hasLength: true },
  { name: 'TEXT', category: 'text' },
  // Dates
  { name: 'DATE', category: 'date' },
  { name: 'TIME', category: 'date' },
  { name: 'TIMESTAMP', category: 'date' },
  { name: 'TIMESTAMPTZ', category: 'date' },
  // Others
  { name: 'BOOLEAN', category: 'boolean' },
  { name: 'UUID', category: 'other' },
  { name: 'BYTEA', category: 'binary' },
  { name: 'JSON', category: 'json' },
  { name: 'JSONB', category: 'json' },
];

// MySQL column types
const MYSQL_TYPES: ColumnType[] = [
  // Integers
  { name: 'TINYINT', category: 'integer' },
  { name: 'SMALLINT', category: 'integer' },
  { name: 'MEDIUMINT', category: 'integer' },
  { name: 'INT', category: 'integer' },
  { name: 'BIGINT', category: 'integer' },
  // Floats
  { name: 'FLOAT', category: 'float' },
  { name: 'DOUBLE', category: 'float' },
  { name: 'DECIMAL', category: 'float', hasPrecision: true },
  // Strings
  { name: 'VARCHAR', category: 'string', hasLength: true },
  { name: 'CHAR', category: 'string', hasLength: true },
  { name: 'TEXT', category: 'text' },
  { name: 'MEDIUMTEXT', category: 'text' },
  { name: 'LONGTEXT', category: 'text' },
  // Dates
  { name: 'DATE', category: 'date' },
  { name: 'TIME', category: 'date' },
  { name: 'DATETIME', category: 'date' },
  { name: 'TIMESTAMP', category: 'date' },
  // Others
  { name: 'BOOLEAN', category: 'boolean' },
  { name: 'BLOB', category: 'binary' },
  { name: 'JSON', category: 'json' },
];

// SQLite column types
const SQLITE_TYPES: ColumnType[] = [
  // Integers
  { name: 'INTEGER', category: 'integer' },
  { name: 'INT', category: 'integer' },
  { name: 'BIGINT', category: 'integer' },
  { name: 'SMALLINT', category: 'integer' },
  { name: 'TINYINT', category: 'integer' },
  // Floats
  { name: 'REAL', category: 'float' },
  { name: 'DOUBLE', category: 'float' },
  { name: 'FLOAT', category: 'float' },
  { name: 'NUMERIC', category: 'float', hasPrecision: true },
  { name: 'DECIMAL', category: 'float', hasPrecision: true },
  // Strings
  { name: 'TEXT', category: 'text' },
  { name: 'VARCHAR', category: 'string', hasLength: true },
  { name: 'CHAR', category: 'string', hasLength: true },
  { name: 'CLOB', category: 'text' },
  // Dates
  { name: 'DATE', category: 'date' },
  { name: 'DATETIME', category: 'date' },
  { name: 'TIMESTAMP', category: 'date' },
  // Others
  { name: 'BOOLEAN', category: 'boolean' },
  { name: 'BLOB', category: 'binary' },
];

// DuckDB column types
const DUCKDB_TYPES: ColumnType[] = [
  // Integers
  { name: 'TINYINT', category: 'integer' },
  { name: 'SMALLINT', category: 'integer' },
  { name: 'INTEGER', category: 'integer' },
  { name: 'BIGINT', category: 'integer' },
  { name: 'HUGEINT', category: 'integer' },
  // Floats
  { name: 'FLOAT', category: 'float' },
  { name: 'DOUBLE', category: 'float' },
  { name: 'DECIMAL', category: 'float', hasPrecision: true },
  // Strings
  { name: 'VARCHAR', category: 'string', hasLength: true },
  { name: 'TEXT', category: 'text' },
  // Dates
  { name: 'DATE', category: 'date' },
  { name: 'TIME', category: 'date' },
  { name: 'TIMESTAMP', category: 'date' },
  { name: 'TIMESTAMPTZ', category: 'date' },
  { name: 'INTERVAL', category: 'date' },
  // Others
  { name: 'BOOLEAN', category: 'boolean' },
  { name: 'UUID', category: 'other' },
  { name: 'BLOB', category: 'binary' },
  { name: 'JSON', category: 'json' },
];

// Driver to column types mapping
export const COLUMN_TYPES: Record<Driver, ColumnType[]> = {
  [Driver.Postgres]: POSTGRES_TYPES,
  [Driver.Mysql]: MYSQL_TYPES,
  [Driver.Mongodb]: [],
  [Driver.Redis]: [],
  [Driver.Sqlite]: SQLITE_TYPES,
  [Driver.Duckdb]: DUCKDB_TYPES,
};

/** Get column types for a driver */
export function getColumnTypes(driver: Driver): ColumnType[] {
  return COLUMN_TYPES[driver] || [];
}

/** Build column definition SQL for a single column */
export function buildColumnSQL(col: ColumnDef, driver: Driver): string {
  let sql = `${quoteIdentifier(col.name, driver)} ${col.type}`;

  // Add length/precision
  if (col.length) {
    sql = `${quoteIdentifier(col.name, driver)} ${col.type}(${col.length})`;
  } else if (col.precision) {
    sql = `${quoteIdentifier(col.name, driver)} ${col.type}(${col.precision}${col.scale ? `, ${col.scale}` : ''})`;
  }

  // Constraints
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
  // MySQL AUTO_INCREMENT
  if (col.isAutoIncrement && driver === Driver.Mysql) {
    sql += ' AUTO_INCREMENT';
  }

  return sql;
}

function quoteIdentifier(identifier: string, driver: Driver): string {
  const driverMeta = getDriverMetadata(driver);
  const { quoteStart, quoteEnd } = driverMeta.identifier;
  let escaped = identifier;
  if (quoteStart === quoteEnd) {
    escaped = identifier.replace(new RegExp(quoteStart, 'g'), `${quoteStart}${quoteStart}`);
  } else {
    escaped = identifier
      .replace(new RegExp(quoteStart, 'g'), `${quoteStart}${quoteStart}`)
      .replace(new RegExp(quoteEnd, 'g'), `${quoteEnd}${quoteEnd}`);
  }
  return `${quoteStart}${escaped}${quoteEnd}`;
}

export function buildQualifiedTableName(
  namespace: NamespaceLike,
  tableName: string,
  driver: Driver
): string {
  if (driver === Driver.Sqlite) {
    return quoteIdentifier(tableName, driver);
  }

  const driverMeta = getDriverMetadata(driver);
  const schema = namespace.schema || undefined;
  const database = namespace.database;

  if (driverMeta.identifier.namespaceStrategy === 'schema' && schema) {
    return `${quoteIdentifier(schema, driver)}.${quoteIdentifier(tableName, driver)}`;
  }

  if (driverMeta.identifier.namespaceStrategy === 'database' && database) {
    return `${quoteIdentifier(database, driver)}.${quoteIdentifier(tableName, driver)}`;
  }

  return quoteIdentifier(tableName, driver);
}

export function buildTruncateTableSQL(
  namespace: NamespaceLike,
  tableName: string,
  driver: Driver
): string {
  return `TRUNCATE TABLE ${buildQualifiedTableName(namespace, tableName, driver)}`;
}

/** Build CREATE TABLE SQL */
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

/** Build DROP TABLE SQL */
export function buildDropTableSQL(
  namespace: NamespaceLike,
  tableName: string,
  driver: Driver
): string {
  const fullName = buildQualifiedTableName(namespace, tableName, driver);

  return `DROP TABLE ${fullName};`;
}
