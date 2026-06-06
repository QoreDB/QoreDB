// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@/lib/transport';
import type { RowData } from './mutations';
import type { Namespace, Value } from './types';

// ============================================
// SANDBOX COMMANDS
// ============================================

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
  useTransaction: boolean = true
): Promise<ApplySandboxResult> {
  return invoke('apply_sandbox_changes', { sessionId, changes, useTransaction });
}
