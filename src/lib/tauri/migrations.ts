// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@/lib/transport';

export type MigrationDirection = 'up' | 'down';
/** `failed` means a run died part-way and the driver could not undo it, so the
 *  schema is neither migrated nor untouched. */
export type MigrationRunStatus = 'applied' | 'pending' | 'rolled_back' | 'failed';

export interface MigrationStatusEntry {
  version: string;
  name: string;
  filename: string;
  status: MigrationRunStatus;
  applied_at: string | null;
  /** Direction of the non-transactional run that failed. */
  failed_direction: MigrationDirection | null;
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
  | 'malformed_version'
  | 'unsplittable_script'
  | 'safety_blocked'
  | 'unsupported_driver'
  | 'partially_applied';

export interface ApplyMigrationResponse {
  success: boolean;
  execution_ms: number;
  error: string | null;
  failed_statement: number | null;
  blocked_reason: MigrationBlockReason | null;
  /** True when re-calling with `force` would get past this refusal. */
  overridable: boolean;
}

const MIGRATION_CONFIRMATION_ACTION = 'apply_migration';

/**
 * Acquires a short-lived, single-use backend acknowledgement. Call this only
 * after the user has accepted the migration confirmation dialog.
 */
export async function requestMigrationConfirmationToken(): Promise<string> {
  const { token } = await invoke<{ token: string; expires_in_secs: number }>(
    'request_confirmation_token',
    { action: MIGRATION_CONFIRMATION_ACTION }
  );
  return token;
}

export async function applyMigration(
  sessionId: string,
  filename: string,
  direction: MigrationDirection,
  database: string,
  confirmationToken?: string,
  force = false
): Promise<ApplyMigrationResponse> {
  return invoke('apply_migration', {
    sessionId,
    filename,
    direction,
    database,
    confirmationToken: confirmationToken ?? null,
    force,
  });
}

/** Per-connection applied/pending status. Null if the workspace is the default. */
export async function getMigrationStatus(
  sessionId: string,
  database?: string
): Promise<MigrationStatusEntry[] | null> {
  return invoke('get_migration_status', { sessionId, database });
}
