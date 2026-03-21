// SPDX-License-Identifier: Apache-2.0

/**
 * DataGrid utility functions
 */

import type { QueryResult, Value } from '@/lib/tauri';
import { estimateByteSizeFromBase64, formatFileSize, isBinaryType } from '@/lib/binaryUtils';

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
 * Convert QueryResult rows to RowData format
 */
export function convertToRowData(result: QueryResult): RowData[] {
  return result.rows.map(row => {
    const data: RowData = {};
    result.columns.forEach((col, idx) => {
      data[col.name] = row.values[idx];
    });
    return data;
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
