// SPDX-License-Identifier: Apache-2.0

import type { ColumnInfo, Row, Value } from '@/lib/tauri/types';

const NUMERIC_TYPE_RE =
  /int|serial|float|double|real|numeric|decimal|number|money|bigint|smallint|tinyint/i;
const SAMPLE_SIZE = 50;

export function toChartNumber(value: Value): number | null {
  if (typeof value === 'number') return Number.isFinite(value) ? value : null;
  if (typeof value === 'string' && value.trim() !== '') {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

/**
 * Columns a Y axis can plot. The declared type wins when it says numeric;
 * otherwise (Mongo, SQLite affinity, untyped drivers) the sampled values decide.
 */
export function numericColumnNames(columns: ColumnInfo[], rows: Row[]): string[] {
  const sample = rows.slice(0, SAMPLE_SIZE);
  return columns
    .map((column, idx) => ({ column, idx }))
    .filter(({ column }) => !column.masked)
    .filter(({ column, idx }) => {
      if (NUMERIC_TYPE_RE.test(column.data_type)) return true;
      const present = sample.map(row => row.values[idx]).filter(v => v !== null && v !== undefined);
      return present.length > 0 && present.every(v => toChartNumber(v) !== null);
    })
    .map(({ column }) => column.name);
}

export function buildChartPoints(
  columns: ColumnInfo[],
  rows: Row[],
  xColumn: string,
  yColumns: string[]
): Record<string, unknown>[] {
  const xIdx = columns.findIndex(c => c.name === xColumn && !c.masked);
  const yIdxs = yColumns.map(y => columns.findIndex(c => c.name === y && !c.masked));
  if (xIdx < 0 || yIdxs.some(idx => idx < 0)) return [];
  return rows.map(row => {
    const point: Record<string, unknown> = { [xColumn]: row.values[xIdx] };
    yColumns.forEach((y, i) => {
      point[y] = toChartNumber(row.values[yIdxs[i]]);
    });
    return point;
  });
}
