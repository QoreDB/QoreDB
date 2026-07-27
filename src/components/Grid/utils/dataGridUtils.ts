// SPDX-License-Identifier: Apache-2.0

import { estimateByteSizeFromBase64, formatFileSize, isBinaryType } from '@/lib/binaryUtils';
import { exactIntText, isExactInt } from '@/lib/query/exactInt';
import type { QueryResult, Value } from '@/lib/tauri';

export type RowData = Record<string, Value>;

/**
 * Format a Value for display.
 * When dataType is provided and identifies a binary column, displays a
 * human-readable size placeholder instead of the raw base64 string.
 */
export function formatValue(value: Value, dataType?: string): string {
  if (value === null) return 'NULL';
  if (dataType && isBinaryType(dataType) && typeof value === 'string' && value.length > 0) {
    const size = estimateByteSizeFromBase64(value);
    return `<binary ${formatFileSize(size)}>`;
  }
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  if (typeof value === 'number') return String(value);
  if (typeof value === 'string') return value;
  if (isExactInt(value)) return exactIntText(value);
  if (typeof value === 'object') {
    if (Array.isArray(value)) return JSON.stringify(value);
    return JSON.stringify(value);
  }
  return String(value);
}

/**
 * Convert QueryResult rows to RowData format.
 *
 * Uses a lightweight Proxy per row to avoid O(N×M) object creation. Instead
 * of copying every value into a new keyed object, each row is a thin Proxy
 * backed by the original values array. Property access (`row[colName]`) is
 * resolved via a shared column-index map at near-zero cost. Only cells that
 * are actually rendered/accessed incur work — critical for virtualized grids
 * where the vast majority of rows are never touched.
 *
 * The Proxy target is a null-prototype plain object (not the array itself)
 * so that `Array.isArray`, `JSON.stringify`, and TanStack Table all treat
 * each row as a regular record, not an array.
 */
export function convertToRowData(result: QueryResult): RowData[] {
  if (result.rows.length === 0) return [];

  const colNames = result.columns.map(c => c.name);
  const colIndices = new Map<string, number>();
  for (let i = 0; i < colNames.length; i++) {
    colIndices.set(colNames[i], i);
  }

  // Store each row's values in a WeakMap keyed by its unique target object.
  // This lets us share a single handler across all rows while each Proxy
  // still resolves to its own values array.
  const valuesStore = new WeakMap<object, Value[]>();

  const handler: ProxyHandler<RowData> = {
    get(target, prop) {
      if (typeof prop === 'string') {
        const idx = colIndices.get(prop);
        if (idx !== undefined) return valuesStore.get(target)![idx];
      }
      return undefined;
    },
    has(_, prop) {
      return typeof prop === 'string' && colIndices.has(prop);
    },
    ownKeys() {
      return colNames;
    },
    getOwnPropertyDescriptor(target, prop) {
      if (typeof prop === 'string') {
        const idx = colIndices.get(prop);
        if (idx !== undefined) {
          return {
            configurable: true,
            enumerable: true,
            value: valuesStore.get(target)![idx],
            writable: false,
          };
        }
      }
      return undefined;
    },
  };

  return result.rows.map(row => {
    const target = Object.create(null) as RowData;
    valuesStore.set(target, row.values);
    return new Proxy(target, handler);
  });
}

/**
 * Characters a cell shows before it becomes an excerpt. Wide enough that no
 * ordinary column is ever cut, narrow enough that a document or a long text
 * cannot put megabytes into the DOM for the handful of characters the column
 * is wide.
 */
export const CELL_PREVIEW_LIMIT = 512;

export interface CellPreview {
  text: string;
  /** True when the cell holds more than `text` shows. */
  truncated: boolean;
}

/**
 * What a grid cell displays.
 *
 * Deliberately separate from `formatValue`, which stays exact because copy,
 * export and filters read it. Only the preview is cut, never the value: the
 * full content stays one click away in the row, and everything that leaves the
 * grid still carries it whole.
 */
export function formatCellPreview(value: Value, dataType?: string): CellPreview {
  const formatted = formatValue(value, dataType);
  if (formatted.length <= CELL_PREVIEW_LIMIT) return { text: formatted, truncated: false };
  return { text: formatted.slice(0, CELL_PREVIEW_LIMIT), truncated: true };
}

export interface RowDataCache {
  source: QueryResult;
  converted: RowData[];
}

/**
 * An infinite scroll appends: the result object is new on every page, but the
 * rows already converted are the same objects. Rebuilding them costs one Proxy
 * per loaded row per page, so the allocation grows with the scroll depth — a
 * hundred pages of a hundred rows means a million proxies instead of ten
 * thousand. Reuse the converted prefix whenever the new result extends the
 * previous one, and fall back to a full conversion when it does not.
 */
export function convertToRowDataIncremental(
  result: QueryResult,
  cache: RowDataCache | null
): RowDataCache {
  const previous = cache?.source;
  const done = previous?.rows.length ?? 0;
  const extendsPrevious =
    cache !== null &&
    previous !== undefined &&
    previous.columns === result.columns &&
    result.rows.length >= done &&
    (done === 0 || result.rows[done - 1] === previous.rows[done - 1]);

  if (!extendsPrevious) {
    return { source: result, converted: convertToRowData(result) };
  }
  if (result.rows.length === done) {
    return { source: result, converted: cache.converted };
  }
  const appended = convertToRowData({ ...result, rows: result.rows.slice(done) });
  return { source: result, converted: cache.converted.concat(appended) };
}

export function escapeCSV(value: string): string {
  if (value.includes(',') || value.includes('"') || value.includes('\n')) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}
