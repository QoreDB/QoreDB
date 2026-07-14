// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@/lib/transport';

export type MigrationDirection = 'up' | 'down';
export type MigrationRunStatus = 'applied' | 'pending' | 'rolled_back';

export interface MigrationStatusEntry {
  version: string;
  name: string;
  filename: string;
  status: MigrationRunStatus;
  applied_at: string | null;
  checksum_mismatch: boolean;
}

export interface ApplyMigrationResponse {
  success: boolean;
  execution_ms: number;
  error: string | null;
  failed_statement: number | null;
}

export async function applyMigration(
  sessionId: string,
  filename: string,
  direction: MigrationDirection,
  database: string,
  acknowledged = false
): Promise<ApplyMigrationResponse> {
  return invoke('apply_migration', { sessionId, filename, direction, database, acknowledged });
}

/** Per-connection applied/pending status. Null if the workspace is the default. */
export async function getMigrationStatus(
  sessionId: string
): Promise<MigrationStatusEntry[] | null> {
  return invoke('get_migration_status', { sessionId });
}
