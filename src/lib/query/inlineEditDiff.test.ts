// SPDX-License-Identifier: BUSL-1.1

import { describe, expect, it } from 'vitest';
import { countChanges, diffLines } from './inlineEditDiff';

describe('diffLines', () => {
  it('marks every line as context when nothing changed', () => {
    const query = 'SELECT id\nFROM users';
    expect(diffLines(query, query)).toEqual([
      { kind: 'context', text: 'SELECT id' },
      { kind: 'context', text: 'FROM users' },
    ]);
  });

  it('keeps the untouched lines and shows the removal before its replacement', () => {
    const before = 'SELECT id\nFROM users\nORDER BY id';
    const after = 'SELECT id\nFROM users\nWHERE active\nORDER BY id';

    expect(diffLines(before, after)).toEqual([
      { kind: 'context', text: 'SELECT id' },
      { kind: 'context', text: 'FROM users' },
      { kind: 'added', text: 'WHERE active' },
      { kind: 'context', text: 'ORDER BY id' },
    ]);
  });

  it('pairs a modified line as a removal followed by an addition', () => {
    const lines = diffLines('SELECT *\nFROM users', 'SELECT id\nFROM users');

    expect(lines).toEqual([
      { kind: 'removed', text: 'SELECT *' },
      { kind: 'added', text: 'SELECT id' },
      { kind: 'context', text: 'FROM users' },
    ]);
  });

  it('handles an empty side', () => {
    expect(diffLines('', 'SELECT 1')).toEqual([
      { kind: 'removed', text: '' },
      { kind: 'added', text: 'SELECT 1' },
    ]);
  });

  it('degrades to a whole-block replacement past the line cap', () => {
    const before = Array.from({ length: 401 }, (_, i) => `line ${i}`).join('\n');
    const after = `${before}\nextra`;
    const lines = diffLines(before, after);

    expect(lines.every(line => line.kind !== 'context')).toBe(true);
    expect(countChanges(lines)).toEqual({ removed: 401, added: 402 });
  });
});

describe('countChanges', () => {
  it('counts added and removed lines', () => {
    const lines = diffLines('a\nb\nc', 'a\nB\nc\nd');
    expect(countChanges(lines)).toEqual({ added: 2, removed: 1 });
  });
});
