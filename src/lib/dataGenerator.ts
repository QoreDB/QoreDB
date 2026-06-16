// SPDX-License-Identifier: BUSL-1.1

import { invoke } from '@/lib/transport';
import type { Namespace } from './tauri';

export interface SeedDataResult {
  /** Generated INSERT script (one or more statements separated by `;\n\n`). */
  sql: string;
  rowCount: number;
  warnings: string[];
}

/**
 * Generates a seed INSERT script for a table (Pro). The script is returned for
 * preview/export/execution — the backend does not touch the database.
 */
export async function generateSeedData(
  sessionId: string,
  namespace: Namespace,
  table: string,
  count: number,
  connectionId?: string
): Promise<SeedDataResult> {
  return invoke('generate_seed_data', { sessionId, namespace, table, count, connectionId });
}
