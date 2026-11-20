// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import type { ColumnInfo, Row } from '@/lib/tauri/types';
import { buildChartPoints, numericColumnNames, toChartNumber } from './chartData';

const col = (name: string, data_type: string, masked = false): ColumnInfo => ({
  name,
  data_type,
  nullable: true,
  masked,
});

describe('chartData', () => {
  const columns = [
    col('month', 'text'),
    col('total', 'numeric'),
    col('count_txt', 'text'),
    col('label', 'text'),
    col('salary', 'integer', true),
  ];
  const rows: Row[] = [
    { values: ['2026-01', '12.5', '3', 'a', null] },
    { values: ['2026-02', null, null, 'b', null] },
  ];

  it('keeps declared or sampled numeric columns and drops masked ones', () => {
    expect(numericColumnNames(columns, rows)).toEqual(['total', 'count_txt']);
  });

  it('keeps gaps as null instead of plotting them as zero', () => {
    expect(buildChartPoints(columns, rows, 'month', ['total'])).toEqual([
      { month: '2026-01', total: 12.5 },
      { month: '2026-02', total: null },
    ]);
    expect(buildChartPoints(columns, rows, 'month', ['salary'])).toEqual([]);
  });

  it('rejects non-numeric values', () => {
    expect(toChartNumber('abc')).toBeNull();
    expect(toChartNumber('')).toBeNull();
    expect(toChartNumber(true)).toBeNull();
    expect(toChartNumber(7)).toBe(7);
  });
});
