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
  duplicate_version: boolean;
  malformed: boolean;
}

/** Machine-readable refusal, so the reason can be translated rather than parsed. */
export type MigrationBlockReason =
  | 'already_applied'
  | 'not_applied'
  | 'already_rolled_back'
  | 'checksum_mismatch'
  | 'concurrent_apply'
  | 'duplicate_version'
  | 'unsplittable_script'
  | 'safety_blocked';

export interface ApplyMigrationResponse {
  success: boolean;
  execution_ms: number;
  error: string | null;
  failed_statement: number | null;
  blocked_reason: MigrationBlockReason | null;
  /** True when re-calling with `force` would get past this refusal. */
  overridable: boolean;
}

export async function applyMigration(
  sessionId: string,
  filename: string,
  direction: MigrationDirection,
  database: string,
  acknowledged = false,
  force = false
): Promise<ApplyMigrationResponse> {
  return invoke('apply_migration', {
    sessionId,
    filename,
    direction,
    database,
    acknowledged,
    force,
  });
}

/** Per-connection applied/pending status. Null if the workspace is the default. */
export async function getMigrationStatus(
  sessionId: string
): Promise<MigrationStatusEntry[] | null> {
  return invoke('get_migration_status', { sessionId });
}
