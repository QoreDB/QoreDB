/**
 * Driver definitions and metadata for QoreDB
 * 
 * This module provides semantic information about each database driver,
 * enabling the UI to adapt terminology and behavior per database type.
 */

export type Driver = 'postgres' | 'mysql' | 'mongodb';

export interface DriverMetadata {
  id: Driver;
  label: string;
  icon: string;
  defaultPort: number;
  // Namespace semantics
  namespaceLabel: string;        // "Schema" for pg, "Database" for mysql/mongo
  namespacePluralLabel: string;  // "Schemas" for pg, "Databases" for mysql/mongo
  collectionLabel: string;       // "Table" for SQL, "Collection" for NoSQL
  collectionPluralLabel: string; // "Tables" for SQL, "Collections" for NoSQL
  // Tree behavior
  treeRootLabel: string;         // What to show as header in DBTree (i18n key)
  createAction: 'schema' | 'database' | 'none';
  // Connection modal
  databaseFieldLabel: string;    // i18n key for the database field in connection modal
  // Capabilities
  supportsSchemas: boolean;
  supportsSQL: boolean;
}

export const DRIVERS: Record<Driver, DriverMetadata> = {
  postgres: {
    id: 'postgres',
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
  },
  mysql: {
    id: 'mysql',
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
  },
  mongodb: {
    id: 'mongodb',
    label: 'MongoDB',
    icon: 'mongodb.png',
    defaultPort: 27017,
    namespaceLabel: 'dbtree.database',
    namespacePluralLabel: 'dbtree.databases',
    collectionLabel: 'dbtree.collection',
    collectionPluralLabel: 'dbtree.collections',
    treeRootLabel: 'dbtree.databasesHeader',
    createAction: 'none',
    databaseFieldLabel: 'connection.database',
    supportsSchemas: false,
    supportsSQL: false,
  },
};

// Helper to get driver metadata with fallback
export function getDriverMetadata(driver: string): DriverMetadata {
  return DRIVERS[driver as Driver] ?? DRIVERS.postgres;
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