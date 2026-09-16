// SPDX-License-Identifier: Apache-2.0

import { executeQuery } from '../tauri/query';
import type { Namespace } from '../tauri/types';

export async function estimateBigQueryScan(
  sessionId: string,
  query: string,
  namespace?: Namespace
): Promise<number | null> {
  const response = await executeQuery(sessionId, `EXPLAIN ${query}`, { namespace });
  if (!response.success) throw new Error(response.error);
  const result = response.result;
  const index = result?.columns.findIndex(column => column.name === 'total_bytes_processed');
  const bytes = index !== undefined && index >= 0 ? result?.rows[0]?.values[index] : null;
  return typeof bytes === 'number' && Number.isFinite(bytes) && bytes >= 0 ? bytes : null;
}
