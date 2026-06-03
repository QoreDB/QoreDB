// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@tauri-apps/api/core';

// ============================================
// MAINTENANCE OPERATIONS
// ============================================

export type MaintenanceOperationType =
  | 'vacuum'
  | 'analyze'
  | 'reindex'
  | 'optimize'
  | 'repair'
  | 'check'
  | 'cluster'
  | 'rebuild_indexes'
  | 'update_statistics'
  | 'compact'
  | 'validate'
  | 'integrity_check'
  | 'change_engine';

export interface MaintenanceOptions {
  full?: boolean;
  with_analyze?: boolean;
  verbose?: boolean;
  index_name?: string;
  target_engine?: string;
}

export interface MaintenanceRequest {
  operation: MaintenanceOperationType;
  options: MaintenanceOptions;
}

export interface MaintenanceOperationInfo {
  operation: MaintenanceOperationType;
  is_heavy: boolean;
  has_options: boolean;
}

export type MaintenanceMessageLevel = 'info' | 'warning' | 'error' | 'status';

export interface MaintenanceMessage {
  level: MaintenanceMessageLevel;
  text: string;
}

export interface MaintenanceResult {
  executed_command: string;
  messages: MaintenanceMessage[];
  execution_time_ms: number;
  success: boolean;
}

export async function listMaintenanceOperations(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  table: string
): Promise<{
  success: boolean;
  operations: MaintenanceOperationInfo[];
  error?: string;
}> {
  return invoke('list_maintenance_operations', { sessionId, database, schema, table });
}

export async function runMaintenance(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  table: string,
  request: MaintenanceRequest,
  acknowledgedDangerous?: boolean
): Promise<{
  success: boolean;
  result?: MaintenanceResult;
  error?: string;
}> {
  return invoke('run_maintenance', {
    sessionId,
    database,
    schema,
    table,
    request,
    acknowledgedDangerous,
  });
}
