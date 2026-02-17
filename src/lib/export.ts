// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@tauri-apps/api/core';
import type { Namespace } from './tauri';

export type ExportFormat = 'csv' | 'json' | 'sql_insert' | 'html';
export type ExportState = 'pending' | 'running' | 'completed' | 'cancelled' | 'failed';

export interface ExportConfig {
  query: string;
  namespace?: Namespace;
  output_path: string;
  format: ExportFormat;
  table_name?: string;
  include_headers: boolean;
  batch_size?: number;
  limit?: number;
}

export interface ExportProgress {
  export_id: string;
  state: ExportState;
  rows_exported: number;
  bytes_written: number;
  elapsed_ms: number;
  rows_per_second?: number | null;
  error?: string;
}

export interface ExportStartResponse {
  export_id: string;
}

export interface ExportCancelResponse {
  success: boolean;
  export_id: string;
  error?: string;
}

export function startExport(
  sessionId: string,
  config: ExportConfig,
  exportId?: string
): Promise<ExportStartResponse> {
  return invoke('start_export', { sessionId, config, exportId });
}

export function cancelExport(exportId: string): Promise<ExportCancelResponse> {
  return invoke('cancel_export', { exportId });
}

export function exportProgressEvent(exportId: string): string {
  return `export_progress:${exportId}`;
}
