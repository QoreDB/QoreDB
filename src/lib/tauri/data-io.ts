// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@tauri-apps/api/core';

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
