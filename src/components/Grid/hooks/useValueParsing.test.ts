// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { parseInputValue } from './useValueParsing';

describe('parseInputValue', () => {
  it('keeps parsing the values a double carries exactly', () => {
    expect(parseInputValue('42', 'int4')).toBe(42);
    expect(parseInputValue('-7', 'bigint')).toBe(-7);
    expect(parseInputValue('1.5', 'numeric(10,2)')).toBe(1.5);
    // The user typing the column's scale must not be pushed onto the text path.
    expect(parseInputValue('1.50', 'numeric(10,2)')).toBe(1.5);
  });

  // Sending these as numbers writes a rounded value to the column. Editing a
  // cell must never be the operation that loses a digit.
  it('wraps a whole number a double would round, so it comes back typed', () => {
    expect(parseInputValue('9007199254740993', 'bigint')).toEqual({
      $qoreInt: '9007199254740993',
    });
    expect(parseInputValue('1409876543210987654', 'int8')).toEqual({
      $qoreInt: '1409876543210987654',
    });
  });

  // A decimal has no envelope yet: it leaves as text and the engine refuses it,
  // which beats writing it rounded.
  it('sends an oversized decimal as text', () => {
    expect(parseInputValue('123456789012345678901.1234567890', 'numeric(40,10)')).toBe(
      '123456789012345678901.1234567890'
    );
  });

  // A double is what these columns store, so the round-trip is theirs, not ours.
  it('still parses approximate columns as numbers', () => {
    expect(parseInputValue('0.1234567890123456789', 'double precision')).toBeTypeOf('number');
    expect(parseInputValue('1e5', 'float8')).toBe(100000);
  });

  it('leaves the other types alone', () => {
    expect(parseInputValue('NULL', 'text')).toBeNull();
    expect(parseInputValue('true', 'boolean')).toBe(true);
    expect(parseInputValue('{"a":1}', 'jsonb')).toEqual({ a: 1 });
    expect(parseInputValue('  ', 'int4')).toBe('');
    expect(parseInputValue('abc', 'int4')).toBe('abc');
  });
});
