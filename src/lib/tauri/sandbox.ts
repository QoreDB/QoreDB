// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@/lib/transport';
import type { RowData } from './mutations';
import type { Namespace, Value } from './types';

export type SandboxChangeType = 'insert' | 'update' | 'delete';

export interface SandboxChangeDto {
  change_type: SandboxChangeType;
  namespace: Namespace;
  table_name: string;
  primary_key?: RowData;
  old_values?: Record<string, Value>;
  new_values?: Record<string, Value>;
}

export interface MigrationScript {
  sql: string;
  statement_count: number;
  warnings: string[];
}

export interface FailedChange {
  index: number;
  error: string;
}

export interface ApplySandboxResult {
  success: boolean;
  applied_count: number;
  applied_indices: number[];
  outcome_unknown: boolean;
  error?: string;
  failed_changes: FailedChange[];
}

export async function generateMigrationSql(
  sessionId: string,
  changes: SandboxChangeDto[]
): Promise<{
  success: boolean;
  script?: MigrationScript;
  error?: string;
}> {
  return invoke('generate_migration_sql', { sessionId, changes });
}

export async function applySandboxChanges(
  sessionId: string,
  changes: SandboxChangeDto[],
  useTransaction: boolean = true,
  acknowledgedDangerous: boolean = false
): Promise<ApplySandboxResult> {
  try {
    return await invoke('apply_sandbox_changes', {
      sessionId,
      changes,
      useTransaction,
      acknowledgedDangerous,
    });
  } catch (error) {
    // A lost IPC response does not establish whether the server committed the batch.
    return {
      success: false,
      applied_count: 0,
      applied_indices: [],
      outcome_unknown: true,
      error: error instanceof Error ? error.message : String(error),
      failed_changes: [],
    };
  }
}
