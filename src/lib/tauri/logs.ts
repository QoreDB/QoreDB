// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@tauri-apps/api/core';
import type {
  Environment,
  MssqlAuthMode,
  SavedConnection,
  VaultResponse,
  VaultStatus,
} from './types';

// ============================================
// LOGS
// ============================================

export async function exportLogs(): Promise<{
  success: boolean;
  filename?: string;
  content?: string;
  error?: string;
}> {
  return invoke('export_logs');
}

export async function getMetrics(): Promise<{
  success: boolean;
  metrics?: {
    total: number;
    failed: number;
    cancelled: number;
    timeouts: number;
    avg_ms?: number;
    max_ms?: number;
  };
  error?: string;
}> {
  return invoke('get_metrics');
}

// ============================================

export async function getVaultStatus(): Promise<VaultStatus> {
  return invoke('get_vault_status');
}

export async function setupMasterPassword(password: string): Promise<VaultResponse> {
  return invoke('setup_master_password', { password });
}

export async function unlockVault(password: string): Promise<VaultResponse> {
  return invoke('unlock_vault', { password });
}

export async function lockVault(): Promise<VaultResponse> {
  return invoke('lock_vault');
}

export async function saveConnection(input: {
  id: string;
  name: string;
  driver: string;
  environment: Environment;
  read_only: boolean;
  host: string;
  port: number;
  username: string;
  password: string;
  database?: string;
  ssl: boolean;
  ssl_mode?: string;
  pool_max_connections?: number;
  pool_min_connections?: number;
  pool_acquire_timeout_secs?: number;
  project_id: string;
  mssql_auth?: MssqlAuthMode;
  /** Distributed cluster name for ClickHouse DDL (`ON CLUSTER`). */
  clickhouse_cluster?: string;
  ssh_tunnel?: {
    host: string;
    port: number;
    username: string;
    auth_type: string;
    password?: string;
    key_path?: string;
    key_passphrase?: string;

    host_key_policy: string;
    proxy_jump?: string;
    connect_timeout_secs: number;
    keepalive_interval_secs: number;
    keepalive_count_max: number;
  };
  proxy?: {
    proxy_type: string;
    host: string;
    port: number;
    username?: string;
    password?: string;
    connect_timeout_secs: number;
  };
}): Promise<VaultResponse> {
  return invoke('save_connection', { input });
}

export async function listSavedConnections(projectId: string): Promise<SavedConnection[]> {
  return invoke('list_saved_connections', { projectId });
}

export async function getConnectionCredentials(
  projectId: string,
  connectionId: string
): Promise<{
  success: boolean;
  password?: string;
  error?: string;
}> {
  return invoke('get_connection_credentials', { projectId, connectionId });
}

export async function deleteSavedConnection(
  projectId: string,
  connectionId: string
): Promise<VaultResponse> {
  return invoke('delete_saved_connection', { projectId, connectionId });
}

export async function duplicateSavedConnection(
  projectId: string,
  connectionId: string
): Promise<{
  success: boolean;
  connection?: SavedConnection;
  error?: string;
}> {
  return invoke('duplicate_saved_connection', { projectId, connectionId });
}
