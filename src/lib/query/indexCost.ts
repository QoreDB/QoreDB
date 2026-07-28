// SPDX-License-Identifier: Apache-2.0

import type { SearchMode, TableIndex, TableSchema } from '@/lib/tauri';

export type { SearchMode };

/** Why a sort or a search cannot use an index. Null when one can. */
export type CostReason = 'leading_wildcard' | 'multi_column' | 'no_index' | null;

/**
 * Columns an index can serve a range or prefix scan on.
 *
 * Only the leading column of each index: a B-tree on `(a, b)` accelerates a
 * predicate on `a`, but not one on `b` alone. Hash indexes answer equality
 * only, so they help neither a prefix search nor an ordering.
 */
export function indexedLeadingColumns(indexes: TableIndex[] | undefined): Set<string> {
  const leading = new Set<string>();
  for (const index of indexes ?? []) {
    const first = index.columns[0];
    if (!first) continue;
    if (index.index_type?.toLowerCase() === 'hash') continue;
    leading.add(first);
  }
  return leading;
}

/**
 * Whether the current search can reach an index, and why not when it cannot.
 *
 * A substring search always scans: the pattern starts with a wildcard, so no
 * B-tree ordering applies. A prefix search over several columns scans too —
 * the engine still has to evaluate the `OR` against every row.
 */
export function searchCost(
  schema: TableSchema | null | undefined,
  columns: string[] | undefined,
  mode: SearchMode
): CostReason {
  if (mode === 'contains') return 'leading_wildcard';
  if (!columns || columns.length === 0) return 'no_index';
  if (columns.length > 1) return 'multi_column';

  const indexed = indexedLeadingColumns(schema?.indexes);
  return indexed.has(columns[0]) ? null : 'no_index';
}

/** Whether sorting on `column` can follow an index instead of sorting the table. */
export function sortCost(schema: TableSchema | null | undefined, column: string): CostReason {
  return indexedLeadingColumns(schema?.indexes).has(column) ? null : 'no_index';
}
