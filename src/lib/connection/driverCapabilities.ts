// SPDX-License-Identifier: Apache-2.0

import { type DataModel, type Driver, getDriverMetadata } from './drivers';

export interface DriverSchemaObjectCapabilities {
  routines: boolean;
  functions: boolean;
  procedures: boolean;
  triggers: boolean;
  events: boolean;
  sequences: boolean;
}

const DRIVER_SCHEMA_OBJECT_CAPABILITIES: Record<Driver, DriverSchemaObjectCapabilities> = {
  postgres: {
    routines: true,
    functions: true,
    procedures: true,
    triggers: true,
    events: false,
    sequences: false,
  },
  mysql: {
    routines: true,
    functions: true,
    procedures: true,
    triggers: true,
    events: true,
    sequences: false,
  },
  mongodb: {
    routines: false,
    functions: false,
    procedures: false,
    triggers: false,
    events: false,
    sequences: false,
  },
  redis: {
    routines: false,
    functions: false,
    procedures: false,
    triggers: false,
    events: false,
    sequences: false,
  },
  sqlite: {
    routines: false,
    functions: false,
    procedures: false,
    triggers: true,
    events: false,
    sequences: false,
  },
  duckdb: {
    routines: false,
    functions: false,
    procedures: false,
    triggers: false,
    events: false,
    sequences: false,
  },
  sqlserver: {
    routines: true,
    functions: true,
    procedures: true,
    triggers: true,
    events: false,
    sequences: false,
  },
  cockroachdb: {
    routines: true,
    functions: true,
    procedures: true,
    triggers: true,
    events: false,
    sequences: false,
  },
  mariadb: {
    routines: true,
    functions: true,
    procedures: true,
    triggers: true,
    events: true,
    sequences: true,
  },
  supabase: {
    routines: true,
    functions: true,
    procedures: true,
    triggers: true,
    events: false,
    sequences: false,
  },
  neon: {
    routines: true,
    functions: true,
    procedures: true,
    triggers: true,
    events: false,
    sequences: false,
  },
  timescaledb: {
    routines: true,
    functions: true,
    procedures: true,
    triggers: true,
    events: false,
    sequences: false,
  },
  clickhouse: {
    routines: false,
    functions: false,
    procedures: false,
    triggers: false,
    events: false,
    sequences: false,
  },
  elasticsearch: {
    routines: false,
    functions: false,
    procedures: false,
    triggers: false,
    events: false,
    sequences: false,
  },
  opensearch: {
    routines: false,
    functions: false,
    procedures: false,
    triggers: false,
    events: false,
    sequences: false,
  },
};

export function isDocumentDatabase(driver: Driver | string): boolean {
  return getDriverMetadata(driver).isDocumentBased;
}

export function isRelationalDatabase(driver: Driver | string): boolean {
  return getDriverMetadata(driver).supportsSQL;
}

export function getDataModel(driver: Driver | string): DataModel {
  return getDriverMetadata(driver).dataModel;
}

export type QueryDialect = 'sql' | 'document' | 'search';

export function getQueryDialect(driver: Driver | string): QueryDialect {
  if (getDataModel(driver) === 'search') return 'search';
  return isDocumentDatabase(driver) ? 'document' : 'sql';
}

/** Driver-agnostic UI labels. Values are i18n keys, not literal strings. */
export interface DriverTerminology {
  /** Label for a single data record: 'row' vs 'document' */
  rowLabel: string;
  /** Label for a collection of records: 'table' vs 'collection' */
  tableLabel: string;
  /** Plural label for records: 'rows' vs 'documents' */
  rowPluralLabel: string;
  /** Plural label for record collections: 'tables' vs 'collections' */
  tablePluralLabel: string;
  /** Action label for inserting: 'insertRow' vs 'insertDocument' */
  insertAction: string;
  /** Action label for updating: 'updateRow' vs 'updateDocument' */
  updateAction: string;
}

export function getTerminology(driver: Driver | string): DriverTerminology {
  const isDocument = isDocumentDatabase(driver);
  return {
    rowLabel: isDocument ? 'terminology.document' : 'terminology.row',
    tableLabel: isDocument ? 'terminology.collection' : 'terminology.table',
    rowPluralLabel: isDocument ? 'terminology.documents' : 'terminology.rows',
    tablePluralLabel: isDocument ? 'terminology.collections' : 'terminology.tables',
    insertAction: isDocument ? 'document.new' : 'rowModal.insertTitle',
    updateAction: isDocument ? 'document.edit' : 'rowModal.updateTitle',
  };
}

export function getSchemaObjectCapabilities(
  driver: Driver | string
): DriverSchemaObjectCapabilities {
  const resolvedDriver = getDriverMetadata(driver).id;
  return DRIVER_SCHEMA_OBJECT_CAPABILITIES[resolvedDriver];
}
