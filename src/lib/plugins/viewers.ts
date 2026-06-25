// SPDX-License-Identifier: Apache-2.0

/**
 * Lookup helper: given a result column (name + declared type) and the
 * aggregated plugin contributions, return the matching `resultViewer` if
 * any. The first contribution that matches wins — the registry already
 * namespaces ids, so two plugins can't shadow each other accidentally.
 */

import type { ResultViewerContribution, ViewerMatch } from './types';

/** Returns the first viewer whose `match` block matches the column. */
export function findViewerFor(
  column: { name?: string | null; columnType?: string | null },
  viewers: ResultViewerContribution[]
): ResultViewerContribution | undefined {
  return viewers.find(v => matches(v.match, column));
}

function matches(
  m: ViewerMatch,
  column: { name?: string | null; columnType?: string | null }
): boolean {
  if (m.columnType && column.columnType) {
    if (m.columnType.toLowerCase() === column.columnType.toLowerCase()) {
      return true;
    }
  }
  if (m.namePattern && column.name) {
    if (globMatches(m.namePattern, column.name)) {
      return true;
    }
  }
  return false;
}

/** Tiny glob matcher — supports only `*` (matches any run of characters).
 *  Backend validation rejects anything fancier so we never see one. */
function globMatches(pattern: string, value: string): boolean {
  // Escape regex metacharacters, then turn `*` into `.*`. We anchor with
  // `^…$` so a pattern like `geom_*` doesn't sneak through on `prefix_geom`.
  const escaped = pattern.replace(/[.+?^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*');
  return new RegExp(`^${escaped}$`).test(value);
}
