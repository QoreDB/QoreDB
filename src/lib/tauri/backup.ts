// SPDX-License-Identifier: Apache-2.0

/**
 * Backup / Restore Tauri API.
 *
 * The backend spawns the official CLI tools (`pg_dump`, `mysqldump`, …) and
 * streams their stdout/stderr as `backup-progress` events keyed by `job_id`.
 * Use {@link listenBackupProgress} to consume them.
 */

import { invoke } from '@/lib/transport';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type BackupTool =
  | 'pg_dump'
  | 'pg_restore'
  | 'psql'
  | 'mysql_dump'
  | 'maria_db_dump'
  | 'mysql'
  | 'mongo_dump'
  | 'mongo_restore'
  | 'sqlite3';

export type BackupMode = 'full' | 'schema_only' | 'data_only';

export type BackupFormat = 'sql' | 'postgres_custom' | 'mongo_archive';

export interface BackupToolInfo {
  tool: BackupTool;
  binary_name: string;
  path: string | null;
  overridden: boolean;
}

export interface BackupOptions {
  driver: string;
  mode: BackupMode;
  format: BackupFormat;
  host: string;
  port: number;
  username?: string | null;
  password?: string | null;
  database?: string | null;
  tables: string[];
  output_path: string;
}

export interface RestoreOptions {
  driver: string;
  host: string;
  port: number;
  username?: string | null;
  password?: string | null;
  database?: string | null;
  input_path: string;
  format: BackupFormat;
}

export interface BackupJobOutcome {
  job_id: string;
  success: boolean;
  exit_code: number | null;
}

export type BackupEvent =
  | { kind: 'started'; job_id: string }
  | { kind: 'log'; stream: 'stdout' | 'stderr'; line: string }
  | { kind: 'completed'; success: boolean; code: number | null };

interface BackupProgressPayload {
  job_id: string;
  event: BackupEvent;
}

export async function detectBackupTools(): Promise<BackupToolInfo[]> {
  const { tools } = await invoke<{ tools: BackupToolInfo[] }>('detect_backup_tools');
  return tools;
}

export async function setBackupToolPath(tool: BackupTool, path: string): Promise<void> {
  await invoke<void>('set_backup_tool_path', { tool, path });
}

export async function startBackup(options: BackupOptions): Promise<BackupJobOutcome> {
  return invoke<BackupJobOutcome>('start_backup', { options });
}

export async function startRestore(options: RestoreOptions): Promise<BackupJobOutcome> {
  return invoke<BackupJobOutcome>('start_restore', { options });
}

/**
 * Signal a running backup or restore job to abort. Resolves to `true` when
 * the job was found and the cancel signal was sent.
 */
export async function cancelBackup(jobId: string): Promise<boolean> {
  return invoke<boolean>('cancel_backup', { jobId });
}

/**
 * Subscribe to live progress events for a single job. The listener is invoked
 * for every stdout/stderr line and for the final completion event.
 */
export async function listenBackupProgress(
  jobId: string,
  onEvent: (event: BackupEvent) => void
): Promise<UnlistenFn> {
  return listen<BackupProgressPayload>('backup-progress', payload => {
    if (payload.payload.job_id === jobId) {
      onEvent(payload.payload.event);
    }
  });
}

/**
 * Subscribe to progress events from all jobs. The listener gets `(jobId, event)`.
 * Useful when the UI wants to track multiple concurrent jobs at once.
 */
export async function listenAllBackupProgress(
  onEvent: (jobId: string, event: BackupEvent) => void
): Promise<UnlistenFn> {
  return listen<BackupProgressPayload>('backup-progress', payload => {
    onEvent(payload.payload.job_id, payload.payload.event);
  });
}
