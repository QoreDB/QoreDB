// SPDX-License-Identifier: Apache-2.0

import type { MigrationDirection, MigrationStatusEntry } from '@/lib/tauri/migrations';

/** Chooses the safe continuation for the history state shown in the list. */
export function nextMigrationDirection(
  status: Pick<MigrationStatusEntry, 'status' | 'failed_direction'> | undefined
): MigrationDirection {
  if (status?.status === 'applied') return 'down';
  if (status?.status === 'failed' && status.failed_direction === 'down') return 'down';
  return 'up';
}
