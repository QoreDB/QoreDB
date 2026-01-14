/**
 * Column types and DDL utilities for QoreDB
 * 
 * Provides driver-aware column type definitions for table creation
 */

import { Driver } from './drivers';

export type ColumnCategory = 'integer' | 'float' | 'string' | 'text' | 'date' | 'binary' | 'json' | 'boolean' | 'other';

export interface ColumnType {
  name: string;
  category: ColumnCategory;
  hasLength?: boolean;     // VARCHAR(n)
  hasPrecision?: boolean;  // DECIMAL(p, s)
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

// Driver to column types mapping
export const COLUMN_TYPES: Record<Driver, ColumnType[]> = {
  postgres: POSTGRES_TYPES,
  mysql: MYSQL_TYPES,
  mongodb: [], // MongoDB is schemaless
};

/** Get column types for a driver */
export function getColumnTypes(driver: Driver): ColumnType[] {
  return COLUMN_TYPES[driver] || [];
}

/** Build column definition SQL for a single column */
export function buildColumnSQL(col: ColumnDef, driver: Driver): string {
  let sql = `"${col.name}" ${col.type}`;
  
  // Add length/precision
  if (col.length) {
    sql = `"${col.name}" ${col.type}(${col.length})`;
  } else if (col.precision) {
    sql = `"${col.name}" ${col.type}(${col.precision}${col.scale ? `, ${col.scale}` : ''})`;
  }
  
  // MySQL uses backticks
  if (driver === 'mysql') {
    sql = sql.replace(/"/g, '`');
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
  if (col.isAutoIncrement && driver === 'mysql') {
    sql += ' AUTO_INCREMENT';
  }
  
  return sql;
}

/** Build CREATE TABLE SQL */
export function buildCreateTableSQL(
  schemaOrDb: string,
  tableName: string,
  columns: ColumnDef[],
  driver: Driver
): string {
  const quote = driver === 'mysql' ? '`' : '"';
  const fullName = driver === 'postgres' 
    ? `${quote}${schemaOrDb}${quote}.${quote}${tableName}${quote}`
    : `${quote}${schemaOrDb}${quote}.${quote}${tableName}${quote}`;
  
  const columnDefs = columns.map(col => buildColumnSQL(col, driver));
  
  return `CREATE TABLE ${fullName} (\n  ${columnDefs.join(',\n  ')}\n);`;
}

/** Build DROP TABLE SQL */
export function buildDropTableSQL(
  schemaOrDb: string,
  tableName: string,
  driver: Driver
): string {
  const quote = driver === 'mysql' ? '`' : '"';
  const fullName = `${quote}${schemaOrDb}${quote}.${quote}${tableName}${quote}`;
  
  return `DROP TABLE ${fullName};`;
}
