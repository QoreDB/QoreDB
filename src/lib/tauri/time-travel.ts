// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@/lib/transport';
import type { Namespace } from './types';

export interface TimelineEvent {
  timestamp: string;
  operation: 'insert' | 'update' | 'delete';
  row_count: number;
  session_id: string;
  connection_name: string | null;
  primary_key: Record<string, unknown> | null;
  entry_id: string;
}

export interface ChangelogEntry {
  id: string;
  timestamp: string;
  session_id: string;
  connection_id: string | null;
  workspace_id?: string | null;
  driver_id: string;
  namespace: Namespace;
  table_name: string;
  operation: 'insert' | 'update' | 'delete';
  primary_key: Record<string, unknown>;
  before: Record<string, unknown> | null;
  after: Record<string, unknown> | null;
  changed_columns: string[];
  connection_name: string | null;
  environment: string;
}

export interface TemporalDiff {
  columns: string[];
  rows: TemporalDiffRow[];
  stats: TemporalDiffStats;
  truncated: boolean;
  incomplete: boolean;
}

export interface TemporalDiffRow {
  primary_key: Record<string, unknown>;
  state_at_t1: Record<string, unknown> | null;
  state_at_t2: Record<string, unknown> | null;
  changed_columns: string[];
  status: 'added' | 'modified' | 'removed';
}

export interface TemporalDiffStats {
  added: number;
  modified: number;
  removed: number;
  total_changes: number;
}

export interface TimeTravelConfig {
  enabled: boolean;
  max_entries: number;
  retention_days: number;
  max_file_size_mb: number;
  excluded_tables: string[];
  production_only: boolean;
}

export interface RollbackSqlResponse {
  success: boolean;
  sql: string | null;
  statements_count: number;
  warnings: string[];
  error: string | null;
}

export async function getTableTimeline(
  sessionId: string,
  database: string,
  schema: string | null,
  tableName: string,
  options?: {
    fromTimestamp?: string;
    toTimestamp?: string;
    operation?: string;
    connectionName?: string;
    environment?: string;
    primaryKeySearch?: string;
    limit?: number;
    offset?: number;
  }
): Promise<{
  success: boolean;
  events: TimelineEvent[];
  total_count: number;
  error: string | null;
}> {
  return invoke('get_table_timeline', {
    sessionId,
    database,
    schema,
    tableName,
    fromTimestamp: options?.fromTimestamp,
    toTimestamp: options?.toTimestamp,
    operation: options?.operation,
    connectionName: options?.connectionName,
    environment: options?.environment,
    primaryKeySearch: options?.primaryKeySearch,
    limit: options?.limit,
    offset: options?.offset,
  });
}

export async function getRowHistory(
  sessionId: string,
  database: string,
  schema: string | null,
  tableName: string,
  primaryKey: Record<string, unknown>,
  limit?: number
): Promise<{
  success: boolean;
  entries: ChangelogEntry[];
  error: string | null;
}> {
  return invoke('get_row_history', { sessionId, database, schema, tableName, primaryKey, limit });
}

export async function computeTemporalDiff(
  sessionId: string,
  database: string,
  schema: string | null,
  tableName: string,
  timestampFrom: string,
  timestampTo: string,
  limit?: number
): Promise<{
  success: boolean;
  diff: TemporalDiff | null;
  error: string | null;
}> {
  return invoke('compute_temporal_diff', {
    sessionId,
    database,
    schema,
    tableName,
    timestampFrom,
    timestampTo,
    limit,
  });
}

export async function getRowStateAt(
  sessionId: string,
  database: string,
  schema: string | null,
  tableName: string,
  primaryKey: Record<string, unknown>,
  timestamp: string
): Promise<{
  success: boolean;
  state: Record<string, unknown> | null;
  exists: boolean;
  error: string | null;
}> {
  return invoke('get_row_state_at', {
    sessionId,
    database,
    schema,
    tableName,
    primaryKey,
    timestamp,
  });
}

export async function generateRollbackSql(
  sessionId: string,
  database: string,
  schema: string | null,
  tableName: string,
  targetTimestamp: string
): Promise<RollbackSqlResponse> {
  return invoke('generate_rollback_sql', {
    sessionId,
    database,
    schema,
    tableName,
    targetTimestamp,
  });
}

export async function generateEntryRollbackSql(
  sessionId: string,
  entryId: string
): Promise<RollbackSqlResponse> {
  return invoke('generate_entry_rollback_sql', { sessionId, entryId });
}

export async function getTimeTravelConfig(): Promise<{
  success: boolean;
  config: TimeTravelConfig;
  error: string | null;
}> {
  return invoke('get_time_travel_config');
}

export async function updateTimeTravelConfig(config: TimeTravelConfig): Promise<{
  success: boolean;
  config: TimeTravelConfig;
  error: string | null;
}> {
  return invoke('update_time_travel_config', { config });
}

async function requestConfirmationToken(action: string): Promise<string> {
  const { token } = await invoke<{ token: string; expires_in_secs: number }>(
    'request_confirmation_token',
    { action }
  );
  return token;
}

export async function clearTableChangelog(
  sessionId: string,
  database: string,
  schema: string | null,
  tableName: string
): Promise<{ success: boolean; error: string | null }> {
  const confirmationToken = await requestConfirmationToken('clear_table_changelog');
  return invoke('clear_table_changelog', {
    sessionId,
    database,
    schema,
    tableName,
    confirmationToken,
  });
}

export async function clearAllChangelog(): Promise<{ success: boolean; error: string | null }> {
  const confirmationToken = await requestConfirmationToken('clear_all_changelog');
  return invoke('clear_all_changelog', { confirmationToken });
}

export async function exportChangelog(
  sessionId: string,
  filter: {
    tableName?: string;
    namespace?: Namespace;
    operation?: string;
    fromTimestamp?: string;
    toTimestamp?: string;
    primaryKeySearch?: string;
    limit?: number;
  }
): Promise<string> {
  return invoke('export_changelog', {
    sessionId,
    filter: {
      table_name: filter.tableName,
      namespace: filter.namespace,
      operation: filter.operation,
      from_timestamp: filter.fromTimestamp,
      to_timestamp: filter.toTimestamp,
      primary_key_search: filter.primaryKeySearch,
      limit: filter.limit,
    },
  });
}
