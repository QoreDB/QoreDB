/**
 * Driver definitions and metadata for QoreDB
 * 
 * This module provides semantic information about each database driver,
 * enabling the UI to adapt terminology and behavior per database type.
 */

export enum Driver {
  Postgres = 'postgres',
  Mysql = 'mysql',
  Mongodb = 'mongodb',
}

/** Query builder functions for driver-specific SQL/commands */
export interface DriverQueryBuilders {
  /** Query to get database/schema total size */
  databaseSizeQuery?: (schemaOrDb: string) => string;
  /** Query to get table size and row count */
  tableSizeQuery?: (schemaOrDb: string, tableName: string) => string;
  /** Query to get index count for a database/schema */
  indexCountQuery?: (schemaOrDb: string) => string;
  /** Query to get table indexes */
  tableIndexesQuery?: (tableName: string) => string;
  /** Query to get maintenance info (vacuum, analyze) */
  maintenanceQuery?: (schemaOrDb: string, tableName: string) => string;
}

export interface IdentifierRules {
  quoteStart: string;
  quoteEnd: string;
  namespaceStrategy: 'schema' | 'database';
}

/** Data model paradigm for database drivers */
export type DataModel = 'relational' | 'document' | 'key-value' | 'graph' | 'time-series';

export interface DriverMetadata {
  id: Driver;
  label: string;
  icon: string;
  defaultPort: number;
  namespaceLabel: string;
  namespacePluralLabel: string;
  collectionLabel: string;
  collectionPluralLabel: string;
  treeRootLabel: string;
  createAction: 'schema' | 'database' | 'none';
  databaseFieldLabel: string;
  supportsSchemas: boolean;
  supportsSQL: boolean;
  dataModel: DataModel;
  isDocumentBased: boolean;
  identifier: IdentifierRules;
  queries: DriverQueryBuilders;
}

export const DRIVERS: Record<Driver, DriverMetadata> = {
  [Driver.Postgres]: {
    id: Driver.Postgres,
    label: 'PostgreSQL',
    icon: 'postgresql.png',
    defaultPort: 5432,
    namespaceLabel: 'dbtree.schema',
    namespacePluralLabel: 'dbtree.schemas',
    collectionLabel: 'dbtree.table',
    collectionPluralLabel: 'dbtree.tables',
    treeRootLabel: 'dbtree.schemasHeader',
    createAction: 'schema',
    databaseFieldLabel: 'connection.databaseInitial',
    supportsSchemas: true,
    supportsSQL: true,
    dataModel: 'relational',
    isDocumentBased: false,
    identifier: {
      quoteStart: '"',
      quoteEnd: '"',
      namespaceStrategy: 'schema',
    },
    queries: {
      databaseSizeQuery: () => 
        `SELECT pg_size_pretty(pg_database_size(current_database())) as size`,
      tableSizeQuery: (schema, table) =>
        `SELECT pg_total_relation_size('"${schema}"."${table}"') as total_bytes,
                pg_size_pretty(pg_total_relation_size('"${schema}"."${table}"')) as size_pretty`,
      indexCountQuery: (schema) =>
        `SELECT COUNT(*) as cnt FROM pg_indexes WHERE schemaname = '${schema}'`,
      tableIndexesQuery: (table) =>
        `SELECT indexname, indexdef FROM pg_indexes WHERE tablename = '${table}'`,
      maintenanceQuery: (schema, table) =>
        `SELECT last_vacuum, last_analyze FROM pg_stat_user_tables 
         WHERE schemaname = '${schema}' AND relname = '${table}'`,
    },
  },
  [Driver.Mysql]: {
    id: Driver.Mysql,
    label: 'MySQL / MariaDB',
    icon: 'mysql.png',
    defaultPort: 3306,
    namespaceLabel: 'dbtree.database',
    namespacePluralLabel: 'dbtree.databases',
    collectionLabel: 'dbtree.table',
    collectionPluralLabel: 'dbtree.tables',
    treeRootLabel: 'dbtree.databasesHeader',
    createAction: 'database',
    databaseFieldLabel: 'connection.database',
    supportsSchemas: false,
    supportsSQL: true,
    dataModel: 'relational',
    isDocumentBased: false,
    identifier: {
      quoteStart: '`',
      quoteEnd: '`',
      namespaceStrategy: 'database',
    },
    queries: {
      databaseSizeQuery: (db) =>
        `SELECT COALESCE(SUM(IFNULL(data_length, 0) + IFNULL(index_length, 0)), 0) as size
         FROM information_schema.tables WHERE table_schema = '${db}'`,
      tableSizeQuery: (db, table) =>
        `SELECT data_length + index_length as total_bytes, table_rows
         FROM information_schema.tables 
         WHERE table_schema = '${db}' AND table_name = '${table}'`,
      indexCountQuery: (db) =>
        `SELECT COUNT(DISTINCT index_name) as cnt 
         FROM information_schema.statistics WHERE table_schema = '${db}'`,
      tableIndexesQuery: (table) => 
        `SHOW INDEX FROM \`${table}\``,
    },
  },
  [Driver.Mongodb]: {
    id: Driver.Mongodb,
    label: 'MongoDB',
    icon: 'mongodb.png',
    defaultPort: 27017,
    namespaceLabel: 'dbtree.database',
    namespacePluralLabel: 'dbtree.databases',
    collectionLabel: 'dbtree.collection',
    collectionPluralLabel: 'dbtree.collections',
    treeRootLabel: 'dbtree.databasesHeader',
    createAction: 'database',
    databaseFieldLabel: 'connection.database',
    supportsSchemas: false,
    supportsSQL: false,
    dataModel: 'document',
    isDocumentBased: true,
    identifier: {
      quoteStart: '"',
      quoteEnd: '"',
      namespaceStrategy: 'database',
    },
    queries: {
    },
  },
};

// Helper to get driver metadata with fallback
export function getDriverMetadata(driver: Driver | string): DriverMetadata {
  return DRIVERS[driver as Driver] ?? DRIVERS[Driver.Postgres];
}

// Legacy exports for backward compatibility
export const DRIVER_LABELS: Record<Driver, string> = Object.fromEntries(
  Object.entries(DRIVERS).map(([k, v]) => [k, v.label])
) as Record<Driver, string>;

export const DRIVER_ICONS: Record<Driver, string> = Object.fromEntries(
  Object.entries(DRIVERS).map(([k, v]) => [k, v.icon])
) as Record<Driver, string>;

export const DEFAULT_PORTS: Record<Driver, number> = Object.fromEntries(
  Object.entries(DRIVERS).map(([k, v]) => [k, v.defaultPort])
) as Record<Driver, number>;
