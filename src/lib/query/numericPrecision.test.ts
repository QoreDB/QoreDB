// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { isExactNumericType, survivesDouble } from './numericPrecision';

describe('isExactNumericType', () => {
  it('covers the integer families', () => {
    for (const type of ['int', 'int2', 'int4', 'int8', 'bigint', 'BIGSERIAL', 'INTEGER']) {
      expect(isExactNumericType(type), type).toBe(true);
    }
  });

  it('covers the arbitrary-precision families', () => {
    for (const type of ['numeric', 'NUMERIC(40,10)', 'decimal(20,2)', 'money']) {
      expect(isExactNumericType(type), type).toBe(true);
    }
  });

  // These columns store a double, so a double is exactly what they hold.
  it('excludes the approximate families', () => {
    for (const type of ['float8', 'double precision', 'real', 'FLOAT']) {
      expect(isExactNumericType(type), type).toBe(false);
    }
  });

  it('excludes non-numeric columns and missing types', () => {
    expect(isExactNumericType('text')).toBe(false);
    expect(isExactNumericType(undefined)).toBe(false);
  });
});

describe('survivesDouble', () => {
  it('accepts values a double carries exactly', () => {
    for (const text of ['0', '1', '-42', '9007199254740991', '1.5', '0.25']) {
      expect(survivesDouble(text), text).toBe(true);
    }
  });

  // The user typing a scale the column carries must not be pushed onto the
  // text path: 1.50 and 1.5 are the same number.
  it('ignores notation that means the same number', () => {
    for (const text of ['1.50', '+7', '007', '-0', '1.0', ' 3 ']) {
      expect(survivesDouble(text), text).toBe(true);
    }
  });

  it('rejects integers past the safe range', () => {
    expect(survivesDouble('9007199254740993')).toBe(false);
    expect(survivesDouble('1409876543210987654')).toBe(false);
    expect(survivesDouble('-9007199254740993')).toBe(false);
  });

  it('rejects decimals with more digits than a double carries', () => {
    expect(survivesDouble('123456789012345678901.1234567890')).toBe(false);
    expect(survivesDouble('0.12345678901234567890123')).toBe(false);
  });

  it('rejects what it cannot compare', () => {
    for (const text of ['1e5', 'abc', '', '1,5', 'NaN', 'Infinity']) {
      expect(survivesDouble(text), text).toBe(false);
    }
  });
});
