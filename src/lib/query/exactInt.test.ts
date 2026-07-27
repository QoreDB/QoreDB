// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import {
  EXACT_INT_KEY,
  exactIntReplacer,
  exactIntText,
  isExactInt,
  toExactValue,
} from './exactInt';

const envelope = { [EXACT_INT_KEY]: '9007199254740993' };

describe('isExactInt', () => {
  it('recognises the envelope', () => {
    expect(isExactInt(envelope)).toBe(true);
  });

  // The engine tries the integer shape before the document one, so anything
  // looser here would swallow a row of real data.
  it('rejects everything that merely resembles it', () => {
    for (const value of [
      null,
      undefined,
      42,
      '9007199254740993',
      {},
      { [EXACT_INT_KEY]: 12 },
      { [EXACT_INT_KEY]: '12', extra: 1 },
      { $qoreIntish: '12' },
      [envelope],
    ]) {
      expect(isExactInt(value), JSON.stringify(value) ?? 'undefined').toBe(false);
    }
  });
});

describe('toExactValue', () => {
  it('keeps ordinary integers as numbers', () => {
    expect(toExactValue('42')).toBe(42);
    expect(toExactValue('-7')).toBe(-7);
    expect(toExactValue('9007199254740991')).toBe(9007199254740991);
  });

  it('wraps what a number would round', () => {
    expect(toExactValue('9007199254740993')).toEqual(envelope);
    expect(toExactValue('1409876543210987654')).toEqual({
      [EXACT_INT_KEY]: '1409876543210987654',
    });
  });

  // The digits the user typed are what has to reach the column, so a value that
  // does not render back identically cannot travel as a number.
  it('wraps anything a round trip would alter', () => {
    expect(toExactValue('007')).toEqual({ [EXACT_INT_KEY]: '007' });
  });
});

describe('exactIntReplacer', () => {
  it('unwraps envelopes to their digits', () => {
    const json = JSON.stringify({ id: envelope, name: 'a' }, exactIntReplacer);
    expect(json).toBe('{"id":"9007199254740993","name":"a"}');
  });

  it('leaves everything else alone', () => {
    const json = JSON.stringify({ a: 1, b: 'x', c: null, d: [1, 2] }, exactIntReplacer);
    expect(json).toBe('{"a":1,"b":"x","c":null,"d":[1,2]}');
  });
});

describe('exactIntText', () => {
  it('returns the digits', () => {
    expect(exactIntText(envelope)).toBe('9007199254740993');
  });
});
