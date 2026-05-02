// SPDX-License-Identifier: Apache-2.0

/**
 * DataGrid utility functions
 */

import { estimateByteSizeFromBase64, formatFileSize, isBinaryType } from '@/lib/binaryUtils';
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
 * Escape a value for CSV format
 */
export function escapeCSV(value: string): string {
  if (value.includes(',') || value.includes('"') || value.includes('\n')) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}
