// SPDX-License-Identifier: BUSL-1.1

import { describe, expect, it } from 'vitest';
import { compareResults, exportDiffAsCSV, exportDiffAsJSON } from './diffUtils';
import type { QueryResult, Value } from './tauri';

function result(values: Value[][], names = ['id', 'name']): QueryResult {
  return {
    columns: names.map(name => ({ name, data_type: 'text', nullable: true })),
    rows: values.map(values => ({ values })),
    execution_time_ms: 0,
  };
}

describe('compareResults', () => {
  it('marks duplicate column names as incomplete', () => {
    const diff = compareResults(
      result([[1, 'before']], ['id', 'id']),
      result([[1, 'after']], ['id', 'id'])
    );
    expect(diff.warnings).toContain('uncomparedColumns');
    expect(diff.incomplete).toBe(true);
  });

  it('preserves a 25000-row duplicate group without losing rows', () => {
    const left = result(Array.from({ length: 25000 }, (_, index) => [1, String(index)]));
    const right = result(Array.from({ length: 24999 }, (_, index) => [1, String(24999 - index)]));
    const diff = compareResults(left, right, ['id']);
    expect(diff.stats).toEqual({
      unchanged: 24999,
      removed: 1,
      added: 0,
      modified: 0,
      total: 25000,
    });
    expect(new Set(diff.rows.map(row => row.rowKey)).size).toBe(25000);
    expect(diff.rows[0].leftCells[1].value).toBe('0');
  });
  it('marks duplicate selected keys as ambiguous while retaining all rows', () => {
    const diff = compareResults(
      result([
        [1, 'a'],
        [1, 'b'],
      ]),
      result([
        [1, 'a'],
        [1, 'b'],
      ]),
      ['id']
    );
    expect(diff.incomplete).toBe(true);
    expect(diff.warnings).toContain('ambiguousKeys');
    expect(diff.stats.unchanged).toBe(2);
    expect(
      compareResults(
        result([
          [1, 'a'],
          [1, 'a'],
        ]),
        result([
          [1, 'a'],
          [1, 'a'],
        ])
      ).incomplete
    ).toBe(false);
  });

  it('marks columns excluded from comparison instead of claiming a complete result', () => {
    const diff = compareResults(
      result([[1, 'secret']], ['id', 'leftOnly']),
      result([[1, 'changed']], ['id', 'rightOnly']),
      ['id']
    );
    expect(diff.incomplete).toBe(true);
    expect(diff.warnings).toContain('uncomparedColumns');
  });

  it('marks positional comparisons with no common column names as incomplete', () => {
    const diff = compareResults(result([[1]], ['left']), result([[1]], ['right']));
    expect(diff.incomplete).toBe(true);
    expect(diff.warnings).toContain('noCommonColumns');
  });

  it('does not silently compare using only part of an unavailable composite key', () => {
    const diff = compareResults(result([[1, 'before']]), result([[1, 'after']]), ['id', 'missing']);
    expect(diff.warnings).toContain('missingKeyColumns');
    expect(diff.stats.modified).toBe(0);
    expect(diff.stats.removed).toBe(1);
    expect(diff.stats.added).toBe(1);
  });

  it('marks masked comparisons and records incompleteness in exports', () => {
    const left = result([[1, '***']]);
    left.columns[1].masked = true;
    const diff = compareResults(left, result([[1, '***']]), ['id']);
    expect(diff.warnings).toContain('maskedColumns');
    expect(JSON.parse(exportDiffAsJSON(diff))).toMatchObject({
      incomplete: true,
      warnings: ['maskedColumns'],
    });
    expect(exportDiffAsCSV(diff).split('\n')[0]).toContain('_comparison_incomplete');
    expect(exportDiffAsCSV(diff).split('\n')[1]).toContain('true');
  });

  it('marks truncated sources explicitly', () => {
    const diff = compareResults(result([[1, 'a']]), result([[1, 'a']]), ['id'], {
      truncated: true,
    });
    expect(diff.warnings).toContain('truncated');
    expect(diff.incomplete).toBe(true);
  });
  it('preserves the multiplicity of identical rows without a selected key', () => {
    const diff = compareResults(
      result([
        [1, 'a'],
        [1, 'a'],
      ]),
      result([[1, 'a']])
    );
    expect(diff.stats).toEqual({ unchanged: 1, removed: 1, added: 0, modified: 0, total: 2 });
    expect(new Set(diff.rows.map(row => row.rowKey)).size).toBe(2);
  });

  it('preserves every duplicate key and matches identical rows before modifications', () => {
    const diff = compareResults(
      result([
        [1, 'a'],
        [1, 'b'],
      ]),
      result([
        [1, 'b'],
        [1, 'c'],
      ]),
      ['id']
    );
    expect(diff.stats).toEqual({ unchanged: 1, removed: 0, added: 0, modified: 1, total: 2 });
    expect(diff.rows.find(row => row.status === 'modified')?.leftCells[1].value).toBe('a');
    expect(diff.rows.find(row => row.status === 'modified')?.rightCells[1].value).toBe('c');
  });

  it('preserves unmatched duplicates on either side', () => {
    expect(
      compareResults(
        result([[1, 'a']]),
        result([
          [1, 'a'],
          [1, 'b'],
          [1, 'c'],
        ]),
        ['id']
      ).stats.added
    ).toBe(2);
    expect(
      compareResults(
        result([
          [1, 'a'],
          [1, 'b'],
          [1, 'c'],
        ]),
        result([[1, 'a']]),
        ['id']
      ).stats.removed
    ).toBe(2);
  });

  it('compares JSON objects independently of property insertion order', () => {
    const left = result([[1, { a: 1, nested: { x: true, y: [null, 'value'] } }]]);
    const right = result([[1, { nested: { y: [null, 'value'], x: true }, a: 1 }]]);
    expect(compareResults(left, right, ['id']).stats.unchanged).toBe(1);
    expect(compareResults(left, right).stats.unchanged).toBe(1);
  });

  it('keeps array order, exact numeric strings, nulls and primitive types distinct', () => {
    const diff = compareResults(
      result([
        [1, ['a', 'b']],
        [2, '9007199254740993'],
        [3, null],
        [4, false],
      ]),
      result([
        [1, ['b', 'a']],
        [2, '9007199254740992'],
        [3, 'NULL'],
        [4, 0],
      ]),
      ['id']
    );
    expect(diff.stats.modified).toBe(4);
  });

  it('supports composite keys and different column positions', () => {
    const diff = compareResults(
      result(
        [
          [1, 'a', 'before'],
          [2, 'a', 'same'],
        ],
        ['id', 'tenant', 'value']
      ),
      result(
        [
          ['same', 'a', 2],
          ['after', 'a', 1],
        ],
        ['value', 'tenant', 'id']
      ),
      ['tenant', 'id']
    );
    expect(diff.stats).toEqual({ unchanged: 1, removed: 0, added: 0, modified: 1, total: 2 });
  });

  it('does not modify the supplied results', () => {
    const left = result([
      [1, 'a'],
      [1, 'b'],
    ]);
    const right = result([
      [1, 'b'],
      [1, 'c'],
    ]);
    const before = JSON.stringify([left, right]);
    compareResults(left, right, ['id']);
    expect(JSON.stringify([left, right])).toBe(before);
  });

  it('retains duplicate rows in both export formats', () => {
    const diff = compareResults(
      result([
        [1, 'a'],
        [1, 'a'],
      ]),
      result([[1, 'a']])
    );
    expect(JSON.parse(exportDiffAsJSON(diff)).rows).toHaveLength(2);
    expect(exportDiffAsCSV(diff).split('\n')).toHaveLength(3);
  });

  it('quotes CSV values containing carriage returns', () => {
    const diff = compareResults(result([]), result([[1, 'a\rb']]));
    expect(exportDiffAsCSV(diff)).toContain('"a\rb"');
  });
});
