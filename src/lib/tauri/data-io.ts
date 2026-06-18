// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@/lib/transport';

// ============================================
// CSV IMPORT
// ============================================

export interface CsvPreviewResponse {
  detected_delimiter: string;
  headers: string[];
  preview_rows: string[][];
  total_lines: number;
}

export interface CsvImportConfig {
  delimiter?: string;
  has_header: boolean;
  null_string?: string;
  on_conflict?: 'skip' | 'abort';
  column_mapping?: Record<number, string>;
}

export interface ImportResponse {
  success: boolean;
  imported_rows: number;
  failed_rows: number;
  errors: string[];
  execution_time_ms: number;
}

export async function previewCsv(
  filePath: string,
  delimiter?: string,
  hasHeader?: boolean,
  previewLimit?: number
): Promise<CsvPreviewResponse> {
  return invoke('preview_csv', {
    filePath,
    delimiter,
    hasHeader,
    previewLimit,
  });
}

export async function importCsv(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  table: string,
  filePath: string,
  config: CsvImportConfig,
  acknowledgedDangerous?: boolean
): Promise<ImportResponse> {
  return invoke('import_csv', {
    sessionId,
    database,
    schema,
    table,
    filePath,
    config,
    acknowledgedDangerous,
  });
}

// ============================================
// SCHEMA EXPORT
// ============================================

export interface SchemaExportOptions {
  include_tables?: boolean;
  include_routines?: boolean;
  include_triggers?: boolean;
  include_events?: boolean;
  include_sequences?: boolean;
}

export interface ExportSchemaResponse {
  success: boolean;
  table_count: number;
  routine_count: number;
  trigger_count: number;
  event_count: number;
  sequence_count: number;
  file_size_bytes: number;
  error?: string;
}

export async function exportSchema(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  filePath: string,
  options: SchemaExportOptions
): Promise<ExportSchemaResponse> {
  return invoke('export_schema', {
    sessionId,
    database,
    schema,
    filePath,
    options,
  });
}

// ============================================
// FULL DATABASE EXPORT (schema + data)
// ============================================

export type DatabaseExportFormat = 'sql' | 'zip';

export interface DatabaseExportOptions {
  include_schema?: boolean;
  include_data?: boolean;
  schema?: SchemaExportOptions;
  /** Restrict the export to these tables (undefined = every table). */
  tables?: string[];
}

export interface DatabaseExportProgress {
  export_id: string;
  state: 'pending' | 'running' | 'completed' | 'cancelled' | 'failed';
  current_table?: string | null;
  tables_done: number;
  tables_total: number;
  rows_exported: number;
  bytes_written: number;
  elapsed_ms: number;
  error?: string | null;
}

export interface DatabaseExportStartResponse {
  export_id: string;
}

export interface DatabaseExportCancelResponse {
  success: boolean;
  export_id: string;
  error?: string;
}

export async function exportDatabaseFull(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  filePath: string,
  format: DatabaseExportFormat,
  options: DatabaseExportOptions,
  exportId?: string
): Promise<DatabaseExportStartResponse> {
  return invoke('export_database_full', {
    sessionId,
    database,
    schema,
    filePath,
    format,
    options,
    exportId,
  });
}

export async function cancelDatabaseExport(
  exportId: string
): Promise<DatabaseExportCancelResponse> {
  return invoke('cancel_database_export', { exportId });
}

export function databaseExportProgressEvent(exportId: string): string {
  return `db_export_progress:${exportId}`;
}
