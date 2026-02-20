// SPDX-License-Identifier: Apache-2.0

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
  Redis = 'redis',
  Sqlite = 'sqlite',
  Duckdb = 'duckdb',
  SqlServer = 'sqlserver',
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
        'SELECT pg_size_pretty(pg_database_size(current_database())) as size',
      tableSizeQuery: (schema, table) =>
        `SELECT pg_total_relation_size('"${schema}"."${table}"') as total_bytes,
                pg_size_pretty(pg_total_relation_size('"${schema}"."${table}"')) as size_pretty`,
      indexCountQuery: schema =>
        `SELECT COUNT(*) as cnt FROM pg_indexes WHERE schemaname = '${schema}'`,
      tableIndexesQuery: table =>
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
      databaseSizeQuery: db =>
        `SELECT COALESCE(SUM(IFNULL(data_length, 0) + IFNULL(index_length, 0)), 0) as size
         FROM information_schema.tables WHERE table_schema = '${db}'`,
      tableSizeQuery: (db, table) =>
        `SELECT data_length + index_length as total_bytes, table_rows
         FROM information_schema.tables 
         WHERE table_schema = '${db}' AND table_name = '${table}'`,
      indexCountQuery: db =>
        `SELECT COUNT(DISTINCT index_name) as cnt 
         FROM information_schema.statistics WHERE table_schema = '${db}'`,
      tableIndexesQuery: table => `SHOW INDEX FROM \`${table}\``,
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
    queries: {},
  },
  [Driver.Redis]: {
    id: Driver.Redis,
    label: 'Redis',
    icon: 'redis.png',
    defaultPort: 6379,
    namespaceLabel: 'dbtree.database',
    namespacePluralLabel: 'dbtree.databases',
    collectionLabel: 'dbtree.key',
    collectionPluralLabel: 'dbtree.keys',
    treeRootLabel: 'dbtree.databasesHeader',
    createAction: 'none',
    databaseFieldLabel: 'connection.databaseIndex',
    supportsSchemas: false,
    supportsSQL: false,
    dataModel: 'key-value',
    isDocumentBased: false,
    identifier: {
      quoteStart: '',
      quoteEnd: '',
      namespaceStrategy: 'database',
    },
    queries: {},
  },
  [Driver.Sqlite]: {
    id: Driver.Sqlite,
    label: 'SQLite',
    icon: 'sqlite.png',
    defaultPort: 0,
    namespaceLabel: 'dbtree.database',
    namespacePluralLabel: 'dbtree.databases',
    collectionLabel: 'dbtree.table',
    collectionPluralLabel: 'dbtree.tables',
    treeRootLabel: 'dbtree.databasesHeader',
    createAction: 'none',
    databaseFieldLabel: 'connection.filePath',
    supportsSchemas: false,
    supportsSQL: true,
    dataModel: 'relational',
    isDocumentBased: false,
    identifier: {
      quoteStart: '"',
      quoteEnd: '"',
      namespaceStrategy: 'database',
    },
    queries: {
      tableSizeQuery: (_, table) =>
        `SELECT page_count * page_size as total_bytes FROM pragma_page_count('${table}'), pragma_page_size()`,
    },
  },
  [Driver.Duckdb]: {
    id: Driver.Duckdb,
    label: 'DuckDB',
    icon: 'duckdb.png',
    defaultPort: 0,
    namespaceLabel: 'dbtree.schema',
    namespacePluralLabel: 'dbtree.schemas',
    collectionLabel: 'dbtree.table',
    collectionPluralLabel: 'dbtree.tables',
    treeRootLabel: 'dbtree.schemasHeader',
    createAction: 'schema',
    databaseFieldLabel: 'connection.filePath',
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
        'SELECT pg_size_pretty(database_size) as size FROM duckdb_databases() WHERE database_name = current_database()',
      tableSizeQuery: (schema, table) =>
        `SELECT estimated_size as total_bytes FROM duckdb_tables() WHERE schema_name = '${schema}' AND table_name = '${table}'`,
      indexCountQuery: schema =>
        `SELECT COUNT(*) as cnt FROM duckdb_indexes() WHERE schema_name = '${schema}'`,
    },
  },
  [Driver.SqlServer]: {
    id: Driver.SqlServer,
    label: 'SQL Server',
    icon: 'sqlserver.png',
    defaultPort: 1433,
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
      quoteStart: '[',
      quoteEnd: ']',
      namespaceStrategy: 'schema',
    },
    queries: {
      databaseSizeQuery: () =>
        `SELECT CAST(SUM(size) * 8.0 / 1024 AS DECIMAL(18,2)) AS size_mb
         FROM sys.database_files`,
      tableSizeQuery: (schema, table) =>
        `SELECT SUM(ps.reserved_page_count) * 8192 AS total_bytes
         FROM sys.dm_db_partition_stats ps
         JOIN sys.tables t ON ps.object_id = t.object_id
         JOIN sys.schemas s ON t.schema_id = s.schema_id
         WHERE s.name = '${schema}' AND t.name = '${table}'`,
      indexCountQuery: schema =>
        `SELECT COUNT(*) AS cnt FROM sys.indexes i
         JOIN sys.tables t ON i.object_id = t.object_id
         JOIN sys.schemas s ON t.schema_id = s.schema_id
         WHERE s.name = '${schema}' AND i.type > 0`,
      tableIndexesQuery: table =>
        `SELECT i.name AS index_name, i.type_desc
         FROM sys.indexes i
         JOIN sys.tables t ON i.object_id = t.object_id
         WHERE t.name = '${table}' AND i.type > 0`,
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
