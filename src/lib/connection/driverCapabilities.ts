// SPDX-License-Identifier: Apache-2.0

/**
 * Driver capability helpers for QoreDB
 *
 * This module provides semantic helper functions for checking driver capabilities,
 * enabling agnostic SQL/NoSQL UI decisions without hardcoding driver checks.
 */

import { type DataModel, type Driver, getDriverMetadata } from './drivers';

/**
 * Fine-grained schema object capabilities by driver.
 * Keep this centralized so all UI surfaces remain consistent.
 */
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
};

/**
 * Check if the driver is document-based (flexible schema, documents as data units)
 * Use this for UI decisions around data display format and terminology
 */
export function isDocumentDatabase(driver: Driver | string): boolean {
  return getDriverMetadata(driver).isDocumentBased;
}

/**
 * Check if the driver supports SQL queries
 * Use this for query editor mode, streaming support, EXPLAIN, etc.
 */
export function isRelationalDatabase(driver: Driver | string): boolean {
  return getDriverMetadata(driver).supportsSQL;
}

/**
 * Get the data model paradigm for a driver
 */
export function getDataModel(driver: Driver | string): DataModel {
  return getDriverMetadata(driver).dataModel;
}

/**
 * Type-safe query dialect derived from driver capabilities
 */
export type QueryDialect = 'sql' | 'document';

/**
 * Get the query dialect for a driver
 */
export function getQueryDialect(driver: Driver | string): QueryDialect {
  return isDocumentDatabase(driver) ? 'document' : 'sql';
}

/**
 * Terminology mappings for driver-agnostic UI labels
 * These map to i18n keys for proper translation
 */
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

/**
 * Get terminology labels for a driver (returns i18n keys)
 */
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

/**
 * Get supported schema objects for a driver.
 */
export function getSchemaObjectCapabilities(
  driver: Driver | string
): DriverSchemaObjectCapabilities {
  const resolvedDriver = getDriverMetadata(driver).id;
  return DRIVER_SCHEMA_OBJECT_CAPABILITIES[resolvedDriver];
}
