// SPDX-License-Identifier: BUSL-1.1

import { describe, expect, it } from 'vitest';
import {
  findInterCellReferences,
  findUnavailableReference,
  invalidateNotebookDependents,
  resolveNotebookReferences,
} from './notebookInterCellRef';
import type { NotebookCell, QoreNotebook } from './notebookTypes';

const cells: NotebookCell[] = [
  {
    id: 'users',
    type: 'sql',
    source: 'SELECT id FROM users',
    config: { label: 'users' },
    executionState: 'success',
    lastResult: {
      type: 'table',
      columns: [{ name: 'id', data_type: 'text', nullable: false }],
      rows: [{ values: ["O'Brien"] }, { values: ['$secret {{secret}}'] }],
    },
  },
];

describe('resolveNotebookReferences', () => {
  it('resolves variables and inter-cell tokens from the original source only', () => {
    expect(
      resolveNotebookReferences(
        'SELECT {{text}}, $text WHERE id IN $users.id',
        {
          text: { name: 'text', type: 'text', currentValue: '$users.id {{secret}} $secret' },
          secret: { name: 'secret', type: 'text', currentValue: 'replacement' },
          users: { name: 'users', type: 'text', currentValue: 'label collision' },
        },
        cells
      )
    ).toBe(
      "SELECT '$users.id {{secret}} $secret', '$users.id {{secret}} $secret' WHERE id IN ('O''Brien', '$secret {{secret}}')"
    );
  });

  it('keeps unavailable references unresolved and preserves dollar escapes', () => {
    expect(
      resolveNotebookReferences('SELECT $missing.id, $users.missing, $$users.id', {}, cells)
    ).toBe('SELECT $missing.id, $users.missing, $$users.id');
    expect(findInterCellReferences('SELECT $$users.id, $users.id')).toEqual([
      { label: 'users', column: 'id' },
    ]);
  });

  it('preserves exact numbers from variables and string-encoded result values', () => {
    const exactCells: NotebookCell[] = [
      {
        ...cells[0],
        lastResult: {
          ...cells[0].lastResult,
          type: 'table',
          rows: [{ values: ['9007199254740993'] }],
        },
      },
    ];
    expect(
      resolveNotebookReferences(
        'SELECT $amount, $users.id',
        { amount: { name: 'amount', type: 'number', currentValue: '9007199254740993' } },
        exactCells
      )
    ).toBe("SELECT 9007199254740993, '9007199254740993'");
  });
});

describe('reference freshness', () => {
  it.each([
    'idle',
    'running',
    'stale',
    'error',
    undefined,
  ] as const)('never substitutes a %s result', executionState => {
    const source = [{ ...cells[0], executionState }];
    expect(findUnavailableReference('SELECT $users.id', source)).toBe('$users.id');
    expect(resolveNotebookReferences('SELECT $users.id', {}, source)).toBe('SELECT $users.id');
  });

  it('rejects missing sources, missing columns and ambiguous labels or columns', () => {
    expect(findUnavailableReference('$missing.id', cells)).toBe('$missing.id');
    expect(findUnavailableReference('$users.missing', cells)).toBe('$users.missing');
    expect(
      findUnavailableReference('$users.id', [...cells, { ...cells[0], id: 'duplicate' }])
    ).toBe('$users.id');
    const result = cells[0].lastResult;
    if (!result?.columns) throw new Error('Missing fixture result');
    expect(
      findUnavailableReference('$users.id', [
        {
          ...cells[0],
          lastResult: { ...result, columns: [...result.columns, ...result.columns] },
        },
      ])
    ).toBe('$users.id');
    expect(findUnavailableReference('SELECT $$missing.id', cells)).toBeUndefined();
  });

  it('refuses masked or truncated results as mutation targets', () => {
    const source = cells[0];
    const result = source.lastResult;
    if (!result?.columns) throw new Error('Missing fixture result');
    const masked = [
      {
        ...source,
        lastResult: {
          ...result,
          columns: result.columns.map(column => ({ ...column, masked: true })),
        },
      },
    ];
    const truncated = [{ ...source, lastResult: { ...result, truncated: true } }];
    for (const values of [masked, truncated]) {
      expect(findUnavailableReference('DELETE FROM users WHERE id = $users.id', values)).toBe(
        '$users.id'
      );
      expect(resolveNotebookReferences('$users.id', {}, values)).toBe('$users.id');
    }
  });

  it('preserves native Mongo field paths while checking known notebook labels', () => {
    expect(findUnavailableReference('{"value":"$document.field"}', cells, 'mongo')).toBeUndefined();
    expect(
      findUnavailableReference('$users.id', [{ ...cells[0], executionState: 'stale' }], 'mongo')
    ).toBe('$users.id');
  });

  it('propagates a change through cycles without invalidating unrelated cells', () => {
    const source = { ...cells[0], source: 'SELECT $other.id' };
    const dependent = {
      ...cells[0],
      id: 'other',
      source: 'SELECT $users.id',
      config: { label: 'other' },
    };
    const unrelated = { ...cells[0], id: 'unrelated', config: { label: 'unrelated' } };
    const nb: QoreNotebook = {
      version: 1,
      metadata: { id: 'nb', title: '', createdAt: '', updatedAt: '' },
      variables: {},
      cells: [source, dependent, unrelated],
    };
    const next = invalidateNotebookDependents(nb, {
      ...nb,
      cells: [{ ...source, source: 'SELECT 2' }, dependent, unrelated],
    });
    expect(next.cells.map(cell => cell.executionState)).toEqual(['stale', 'stale', 'success']);
    expect(nb.cells[0].executionState).toBe('success');
  });

  it('keeps valid results when only display settings or order change', () => {
    const nb: QoreNotebook = {
      version: 1,
      metadata: { id: 'nb', title: '', createdAt: '', updatedAt: '' },
      variables: {},
      cells,
    };
    const next = invalidateNotebookDependents(nb, {
      ...nb,
      cells: [{ ...cells[0], config: { ...cells[0].config, collapsed: true } }],
    });
    expect(next.cells[0].executionState).toBe('success');
  });
});
