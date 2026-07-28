// SPDX-License-Identifier: Apache-2.0

import type { Row, Value } from '@/lib/tauri';

/**
 * Rows the browser keeps for one tab, in payload bytes. The engine's own rows
 * dominate a tab's footprint, but the grid adds a proxy per row and the webview
 * a node per visible cell, so the payload ceiling sits well under the 250 MB the
 * tab as a whole must respect.
 */
export const TAB_PAYLOAD_BUDGET_BYTES = 128 * 1024 * 1024;

// Deep enough for the documents engines actually return, shallow enough that a
// pathological structure cannot exhaust the stack.
const MAX_DEPTH = 32;
// Object header plus one slot; the point is to charge containers for something,
// not to model an allocator.
const OVERHEAD = 16;

function valueBytes(value: Value, depth: number): number {
  if (value === null || value === undefined) return OVERHEAD;
  if (typeof value === 'string') return OVERHEAD + value.length * 2;
  if (typeof value !== 'object') return OVERHEAD;
  if (depth >= MAX_DEPTH) return OVERHEAD;

  let bytes = OVERHEAD;
  if (Array.isArray(value)) {
    for (const item of value) bytes += valueBytes(item as Value, depth + 1);
    return bytes;
  }
  for (const [key, item] of Object.entries(value)) {
    bytes += OVERHEAD + key.length * 2 + valueBytes(item as Value, depth + 1);
  }
  return bytes;
}

/**
 * Walks the values rather than serializing them: a page holding a multi-megabyte
 * cell is exactly the page whose size must be known, and `JSON.stringify` would
 * allocate a copy of it to find out.
 */
export function estimatePayloadBytes(rows: Row[]): number {
  let bytes = 0;
  for (const row of rows) {
    bytes += OVERHEAD;
    for (const value of row.values) bytes += valueBytes(value, 0);
  }
  return bytes;
}
