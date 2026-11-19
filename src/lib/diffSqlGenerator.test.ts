// SPDX-License-Identifier: BUSL-1.1

import { describe, expect, it } from 'vitest';
import { Driver } from '@/lib/connection/drivers';
import { generateDiffSqlPatch } from './diffSqlGenerator';
import { compareResults } from './diffUtils';
import type { QueryResult } from './tauri';

function makeResult(columns: string[], rows: Array<Array<unknown>>, maskedIndex = -1): QueryResult {
  return {
    columns: columns.map((name, i) => ({
      name,
      data_type: 'text',
      nullable: true,
      masked: i === maskedIndex,
    })),
    rows: rows.map(values => ({ values: values as QueryResult['rows'][number]['values'] })),
    execution_time_ms: 1,
  };
}

describe('generateDiffSqlPatch', () => {
  it('generates DELETE, UPDATE (changed columns only), and INSERT statements', () => {
    const left = makeResult(
      ['id', 'name', 'role'],
      [
        [1, 'Alice', 'user'],
        [2, "O'Brien", 'admin'],
      ]
    );
    const right = makeResult(
      ['id', 'name', 'role'],
      [
        [1, 'Alice', 'admin'],
        [3, 'Carol', null],
      ]
    );

    const diff = compareResults(left, right, ['id']);
    const patch = generateDiffSqlPatch(diff, {
      tableName: 'users',
      namespace: { database: 'app', schema: 'public' },
      dialect: Driver.Postgres,
      keyColumns: ['id'],
    });

    expect(patch.statementCount).toBe(3);
    expect(patch.sql).toContain('DELETE FROM "public"."users" WHERE "id" = 2;');
    expect(patch.sql).toContain('UPDATE "public"."users" SET "role" = \'admin\' WHERE "id" = 1;');
    expect(patch.sql).toContain(
      'INSERT INTO "public"."users" ("id", "name", "role") VALUES (3, \'Carol\', NULL);'
    );
  });

  it('preserves $qoreInt exact integers and MySQL backtick quoting', () => {
    const left = makeResult(['id', 'label'], [[{ $qoreInt: '9007199254740993' }, 'old']]);
    const right = makeResult(['id', 'label'], [[{ $qoreInt: '9007199254740993' }, 'new']]);
    const diff = compareResults(left, right, ['id']);

    const patch = generateDiffSqlPatch(diff, {
      tableName: 'orders',
      namespace: { database: 'shop' },
      dialect: Driver.Mysql,
      keyColumns: ['id'],
    });

    expect(patch.sql).toBe(
      "-- QoreDB Visual Data Diff SQL Patch (1 statement(s))\nUPDATE `shop`.`orders` SET `label` = 'new' WHERE `id` = 9007199254740993;\n"
    );
  });

  it('refuses when masked columns are present', () => {
    const left = makeResult(['id', 'email'], [[1, '***']], 1);
    const right = makeResult(['id', 'email'], [[1, 'a@b.com']]);
    const diff = compareResults(left, right, ['id']);

    expect(() => generateDiffSqlPatch(diff, { tableName: 'users', keyColumns: ['id'] })).toThrow(
      /masked columns/i
    );
  });

  it('refuses when keys are ambiguous or missing', () => {
    const left = makeResult(
      ['id', 'name'],
      [
        [1, 'A'],
        [1, 'B'],
      ]
    );
    const right = makeResult(['id', 'name'], [[1, 'A']]);
    const diff = compareResults(left, right, ['id']);

    expect(() => generateDiffSqlPatch(diff, { tableName: 'users', keyColumns: ['id'] })).toThrow(
      /ambiguous/i
    );

    const cleanDiff = compareResults(
      makeResult(['id', 'name'], [[1, 'A']]),
      makeResult(['id', 'name'], [[1, 'B']]),
      ['id']
    );
    expect(() =>
      generateDiffSqlPatch(cleanDiff, {
        tableName: 'users',
        keyColumns: ['id'],
        keyWarning: 'Missing primary key',
      })
    ).toThrow(/ambiguous/i);
  });
});
