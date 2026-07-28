// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import type { QueryResult } from '@/lib/tauri';
import {
  CELL_PREVIEW_LIMIT,
  convertToRowDataIncremental,
  formatCellPreview,
  formatValue,
  type RowDataCache,
} from './dataGridUtils';

const columns = [
  { name: 'id', data_type: 'int8', nullable: false },
  { name: 'label', data_type: 'text', nullable: true },
];

function page(rows: { values: [number, string] }[]): QueryResult {
  return { columns, rows, execution_time_ms: 0 } as unknown as QueryResult;
}

function row(id: number) {
  return { values: [id, `row-${id}`] as [number, string] };
}

function ids(cache: RowDataCache) {
  return cache.converted.map(r => r.id);
}

describe('convertToRowDataIncremental', () => {
  it('converts every row on a cold cache', () => {
    const cache = convertToRowDataIncremental(page([row(1), row(2)]), null);
    expect(ids(cache)).toEqual([1, 2]);
  });

  it('reuses the converted prefix when the result is extended', () => {
    const first = [row(1), row(2)];
    const cold = convertToRowDataIncremental(page(first), null);
    const warm = convertToRowDataIncremental(page([...first, row(3)]), cold);

    expect(ids(warm)).toEqual([1, 2, 3]);
    expect(warm.converted[0]).toBe(cold.converted[0]);
    expect(warm.converted[1]).toBe(cold.converted[1]);
  });

  it('returns the same array when nothing was appended', () => {
    const rows = [row(1), row(2)];
    const cold = convertToRowDataIncremental(page(rows), null);
    const warm = convertToRowDataIncremental(page([...rows]), cold);

    expect(warm.converted).toBe(cold.converted);
  });

  // A reload replaces the rows in place, so the prefix is only reusable when
  // the underlying objects are the same ones.
  it('rebuilds when the rows are different objects', () => {
    const cold = convertToRowDataIncremental(page([row(1), row(2)]), null);
    const warm = convertToRowDataIncremental(page([row(9), row(2), row(3)]), cold);

    expect(ids(warm)).toEqual([9, 2, 3]);
    expect(warm.converted[0]).not.toBe(cold.converted[0]);
  });

  it('rebuilds when the columns changed', () => {
    const rows = [row(1)];
    const cold = convertToRowDataIncremental(page(rows), null);
    const renamed = {
      columns: [
        { name: 'ref', data_type: 'int8', nullable: false },
        { name: 'label', data_type: 'text', nullable: true },
      ],
      rows: [...rows, row(2)],
      execution_time_ms: 0,
    } as unknown as QueryResult;

    const warm = convertToRowDataIncremental(renamed, cold);
    expect(warm.converted.map(r => r.ref)).toEqual([1, 2]);
    expect(warm.converted[0].id).toBeUndefined();
  });

  it('rebuilds when rows were removed', () => {
    const rows = [row(1), row(2), row(3)];
    const cold = convertToRowDataIncremental(page(rows), null);
    const warm = convertToRowDataIncremental(page(rows.slice(0, 2)), cold);

    expect(ids(warm)).toEqual([1, 2]);
  });

  it('matches a full conversion across a long walk', () => {
    let rows: ReturnType<typeof row>[] = [];
    let cache: RowDataCache | null = null;
    for (let p = 0; p < 20; p++) {
      rows = rows.concat(Array.from({ length: 5 }, (_, i) => row(p * 5 + i)));
      cache = convertToRowDataIncremental(page(rows), cache);
    }
    const cold = convertToRowDataIncremental(page(rows), null);

    expect(ids(cache as RowDataCache)).toEqual(ids(cold));
    expect(cache?.converted.map(r => r.label)).toEqual(cold.converted.map(r => r.label));
  });
});

describe('formatCellPreview', () => {
  it('leaves ordinary values whole', () => {
    expect(formatCellPreview('hello')).toEqual({ text: 'hello', truncated: false });
    expect(formatCellPreview(null)).toEqual({ text: 'NULL', truncated: false });
    expect(formatCellPreview(42)).toEqual({ text: '42', truncated: false });
  });

  it('cuts a long text and says so', () => {
    const preview = formatCellPreview('x'.repeat(CELL_PREVIEW_LIMIT + 1));
    expect(preview.truncated).toBe(true);
    expect(preview.text).toHaveLength(CELL_PREVIEW_LIMIT);
  });

  it('keeps a value of exactly the limit whole', () => {
    const preview = formatCellPreview('x'.repeat(CELL_PREVIEW_LIMIT));
    expect(preview.truncated).toBe(false);
  });

  it('cuts a large document too', () => {
    const document = Object.fromEntries(
      Array.from({ length: 200 }, (_, i) => [`field_${i}`, 'value'])
    );
    const preview = formatCellPreview(document);
    expect(preview.truncated).toBe(true);
    expect(preview.text).toHaveLength(CELL_PREVIEW_LIMIT);
  });

  // Copy, export and filters read formatValue, and none of them may ship an
  // excerpt in place of the value.
  it('does not affect the exact formatter', () => {
    const long = 'x'.repeat(CELL_PREVIEW_LIMIT + 100);
    expect(formatValue(long)).toHaveLength(CELL_PREVIEW_LIMIT + 100);
  });
});
