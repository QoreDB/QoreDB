// SPDX-License-Identifier: Apache-2.0

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { PaginatedQueryResult, Value } from '@/lib/tauri';
import { invoke } from '@/lib/transport';
import { estimatePayloadBytes } from './payloadSize';
import {
  prepareTableRowUpdate,
  reconcileTableRowUpdate,
  type TableCellUpdate,
  type TableRowUpdateScope,
  updateTableRow,
} from './tableRowUpdate';

vi.mock('@/lib/transport', () => ({ invoke: vi.fn() }));
const mockedInvoke = vi.mocked(invoke);
beforeEach(() => mockedInvoke.mockReset());

const columns = [
  { name: 'tenant', data_type: 'text', nullable: false },
  { name: 'id', data_type: 'bigint', nullable: false },
  { name: 'rank', data_type: 'int', nullable: false },
  { name: 'name', data_type: 'text', nullable: true },
];
const primaryKey = ['tenant', 'id'];
const namespace = { database: 'fixture', schema: 'public' };

function fixture() {
  const rows = Array.from({ length: 500 }, (_, i) => ({ values: ['a::b', i, i % 3, `name-${i}`] }));
  const scope: TableRowUpdateScope = {
    rows,
    columns,
    primaryKey,
    sortColumn: 'rank',
    orderingGuarantee: 'stable',
    filters: [{ column: 'tenant', operator: 'eq', value: 'a::b' }],
  };
  const update: TableCellUpdate = {
    primaryKey: { tenant: 'a::b', id: 234 },
    column: 'name',
    value: 'typed',
  };
  const target = prepareTableRowUpdate(scope, update);
  if (!target) throw new Error('Fixture must be eligible');
  return { scope, update, target };
}

function readback(values: Value[], strategy: 'offset' | 'keyset' = 'keyset'): PaginatedQueryResult {
  return {
    result: { columns, rows: [{ values }], execution_time_ms: 1 },
    page: 1,
    page_size: 2,
    has_more: false,
    next_cursor: null,
    total_rows: null,
    total_rows_source: null,
    total_rows_as_of: null,
    pagination_strategy: strategy,
    ordering_guarantee: 'stable',
  };
}

describe('confirmed table row reconciliation', () => {
  it.each([
    'offset',
    'keyset',
  ] as const)('keeps five pages in %s order and uses the full server row', strategy => {
    const { scope, target } = fixture();
    const fresh = readback(['a::b', 234, 0, 'TRIGGER NORMALIZED'], strategy);
    const bytes = estimatePayloadBytes(scope.rows);
    const result = reconcileTableRowUpdate(scope.rows, target, fresh, bytes);

    expect(result?.rows).toHaveLength(500);
    expect(result?.rows[234]).toBe(fresh.result.rows[0]);
    expect(result?.rows.map(row => row.values[1])).toEqual(scope.rows.map(row => row.values[1]));
    expect(result?.rows[233]).toBe(scope.rows[233]);
    expect(result?.rows[499]).toBe(scope.rows[499]);
    expect(scope.rows[234].values[3]).toBe('name-234');
    expect(result?.payloadBytes).toBe(estimatePayloadBytes(result?.rows ?? []));
  });

  it('preserves a sixth page that arrived while the mutation was pending', () => {
    const { scope, target } = fixture();
    const appended = { values: ['a::b', 501, 0, 'new page'] };
    const currentRows = [...scope.rows, appended];
    const result = reconcileTableRowUpdate(
      currentRows,
      target,
      readback(['a::b', 234, 0, 'saved']),
      estimatePayloadBytes(currentRows)
    );
    expect(result?.rows).toHaveLength(501);
    expect(result?.rows[500]).toBe(appended);
    expect(result?.rows[234].values[3]).toBe('saved');
  });

  it('matches exact integer and composite keys without rounding or separator collisions', () => {
    const rows = [
      { values: ['a::b', { $qoreInt: '9007199254740993' }, 0, 'old'] },
      { values: ['a::b', { $qoreInt: '9007199254740992' }, 0, 'other'] },
    ];
    const { scope, update } = fixture();
    const target = prepareTableRowUpdate(
      { ...scope, rows },
      {
        ...update,
        primaryKey: { tenant: 'a::b', id: { $qoreInt: '9007199254740993' } },
      }
    );
    expect(target).not.toBeNull();
    if (!target) return;
    const result = reconcileTableRowUpdate(
      rows,
      target,
      readback(['a::b', { $qoreInt: '9007199254740993' }, 0, 'saved']),
      estimatePayloadBytes(rows)
    );
    expect(result?.rows[0].values[3]).toBe('saved');
    expect(result?.rows[1]).toBe(rows[1]);
  });

  it.each([
    'id',
    'tenant',
    'rank',
  ])('reloads when the edited column affects identity, order or filters: %s', column => {
    const { scope, update } = fixture();
    expect(prepareTableRowUpdate(scope, { ...update, column })).toBeNull();
  });

  it('rejects active search, unstable order, missing keys and masked identities', () => {
    const { scope, update } = fixture();
    for (const changes of [
      { searchTerm: 'name' },
      { orderingGuarantee: 'none' as const },
      { primaryKey: [] },
      { columns: columns.map(c => ({ ...c, masked: c.name === 'id' })) },
      { columns: columns.map(c => ({ ...c, masked: c.name === 'name' })) },
    ])
      expect(prepareTableRowUpdate({ ...scope, ...changes }, update)).toBeNull();
  });

  it.each([
    null,
    { hidden: true },
    { $qoreInt: 'invalid' },
    Number.NaN,
    9007199254740992,
  ])('refuses an unusable key: %j', id => {
    const { scope, update } = fixture();
    expect(
      prepareTableRowUpdate(scope, { ...update, primaryKey: { tenant: 'a::b', id } })
    ).toBeNull();
  });

  it.each([
    ['a::b', 999, 0, 'saved'],
    ['a::b', 234, 2, 'saved'],
    ['moved tenant', 234, 0, 'saved'],
  ])('reloads when a trigger changes a protected value: %j', (...values) => {
    const { scope, target } = fixture();
    expect(
      reconcileTableRowUpdate(
        scope.rows,
        target,
        readback(values),
        estimatePayloadBytes(scope.rows)
      )
    ).toBeNull();
  });

  it('rejects a missing, ambiguous, partial or differently masked canonical result', () => {
    const { scope, target } = fixture();
    const fresh = readback(['a::b', 234, 0, 'saved']);
    const variants = [
      { ...fresh, result: { ...fresh.result, rows: [] } },
      { ...fresh, result: { ...fresh.result, rows: [...fresh.result.rows, ...fresh.result.rows] } },
      { ...fresh, has_more: true },
      { ...fresh, result: { ...fresh.result, rows: [{ values: ['a::b', 234] }] } },
      {
        ...fresh,
        result: { ...fresh.result, columns: columns.map(c => ({ ...c, masked: true })) },
      },
    ];
    for (const candidate of variants) {
      expect(
        reconcileTableRowUpdate(scope.rows, target, candidate, estimatePayloadBytes(scope.rows))
      ).toBeNull();
    }
  });

  it('rejects a row already replaced or removed, and a duplicate key on an appended page', () => {
    const { scope, target } = fixture();
    const fresh = readback(['a::b', 234, 0, 'saved']);
    for (const currentRows of [
      scope.rows.filter((_, i) => i !== 234),
      scope.rows.map((row, i) => (i === 234 ? { values: [...row.values] } : row)),
      [...scope.rows, scope.rows[234]],
    ]) {
      expect(
        reconcileTableRowUpdate(currentRows, target, fresh, estimatePayloadBytes(currentRows))
      ).toBeNull();
    }
  });

  it('falls back when the confirmed value would exceed the retained payload budget', () => {
    const { scope, target } = fixture();
    const bytes = estimatePayloadBytes(scope.rows);
    expect(
      reconcileTableRowUpdate(
        scope.rows,
        target,
        readback(['a::b', 234, 0, 'x'.repeat(500)]),
        bytes,
        bytes
      )
    ).toBeNull();
  });
});

describe('one write followed by canonical read', () => {
  function view() {
    return { isCurrent: vi.fn(() => true), replace: vi.fn(() => true), reload: vi.fn() };
  }
  const success = {
    success: true,
    result: { columns: [], rows: [], execution_time_ms: 0, affected_rows: 1 },
  };

  it('forwards production acknowledgement and rereads by every key with no cache or count', async () => {
    const { update, target } = fixture();
    const current = view();
    mockedInvoke
      .mockResolvedValueOnce(success)
      .mockResolvedValueOnce({ success: true, result: readback(['a::b', 234, 0, 'saved']) });
    expect(
      await updateTableRow('session-a', namespace, 'items', update, target, current, true)
    ).toBe(success);
    expect(mockedInvoke).toHaveBeenNthCalledWith(1, 'update_row', {
      sessionId: 'session-a',
      database: 'fixture',
      schema: 'public',
      table: 'items',
      primaryKey: { columns: update.primaryKey },
      data: { columns: { name: 'typed' } },
      acknowledgedDangerous: true,
    });
    expect(mockedInvoke).toHaveBeenNthCalledWith(2, 'query_table', {
      sessionId: 'session-a',
      namespace,
      table: 'items',
      bypassCache: true,
      options: {
        page: 1,
        page_size: 2,
        count_mode: 'none',
        filters: [
          { column: 'tenant', operator: 'eq', value: 'a::b' },
          { column: 'id', operator: 'eq', value: 234 },
        ],
      },
    });
    expect(current.replace).toHaveBeenCalledOnce();
    expect(current.reload).not.toHaveBeenCalled();
  });

  it('does not reread or change the view after a refused write', async () => {
    const { update, target } = fixture();
    const current = view();
    const failure = { success: false, error: 'Read-only connection' };
    mockedInvoke.mockResolvedValueOnce(failure);
    expect(await updateTableRow('session', namespace, 'items', update, target, current)).toBe(
      failure
    );
    expect(mockedInvoke).toHaveBeenCalledOnce();
    expect(current.replace).not.toHaveBeenCalled();
    expect(current.reload).not.toHaveBeenCalled();
  });

  it.each([
    'rejected',
    'unsuccessful',
    'missing',
    'unsafe',
  ])('reloads after a %s read without repeating or misreporting the write', async outcome => {
    const { update, target } = fixture();
    const current = view();
    mockedInvoke.mockResolvedValueOnce(success);
    if (outcome === 'rejected') mockedInvoke.mockRejectedValueOnce(new Error('read timeout'));
    else if (outcome === 'unsafe') {
      mockedInvoke.mockResolvedValueOnce({
        success: true,
        result: readback(['a::b', 234, 0, 'saved']),
      });
      current.replace.mockReturnValue(false);
    } else mockedInvoke.mockResolvedValueOnce({ success: outcome === 'missing' });
    expect(await updateTableRow('session', namespace, 'items', update, target, current)).toBe(
      success
    );
    expect(mockedInvoke.mock.calls.map(([command]) => command)).toEqual([
      'update_row',
      'query_table',
    ]);
    expect(current.reload).toHaveBeenCalledOnce();
  });

  it('reloads ineligible edits without an unnecessary key read', async () => {
    const { update } = fixture();
    const current = view();
    mockedInvoke.mockResolvedValueOnce(success);
    await updateTableRow('session', namespace, 'items', update, null, current);
    expect(mockedInvoke).toHaveBeenCalledOnce();
    expect(current.reload).toHaveBeenCalledOnce();
  });

  it.each([
    'mutation',
    'read',
  ])('ignores a late %s completion after the view changes or unmounts', async stage => {
    const { update, target } = fixture();
    const current = view();
    mockedInvoke.mockImplementationOnce(async () => {
      if (stage === 'mutation') current.isCurrent.mockReturnValue(false);
      return success;
    });
    mockedInvoke.mockImplementationOnce(async () => {
      current.isCurrent.mockReturnValue(false);
      return { success: true, result: readback(['a::b', 234, 0, 'late']) };
    });
    expect(await updateTableRow('session', namespace, 'items', update, target, current)).toBe(
      success
    );
    expect(current.replace).not.toHaveBeenCalled();
    expect(current.reload).not.toHaveBeenCalled();
    expect(mockedInvoke).toHaveBeenCalledTimes(stage === 'mutation' ? 1 : 2);
  });

  it('does not replace the view if a late read rejects after navigation', async () => {
    const { update, target } = fixture();
    const current = view();
    mockedInvoke.mockResolvedValueOnce(success).mockImplementationOnce(async () => {
      current.isCurrent.mockReturnValue(false);
      throw new Error('old session closed');
    });
    await updateTableRow('session', namespace, 'items', update, target, current);
    expect(current.reload).not.toHaveBeenCalled();
  });
});
