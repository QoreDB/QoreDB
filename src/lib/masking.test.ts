// SPDX-License-Identifier: BUSL-1.1

import { describe, expect, it } from 'vitest';
import {
  EMPTY_MASKING,
  findRule,
  normalizeMasking,
  removesMasking,
  withoutRule,
  withRule,
} from './masking';
import type { ConnectionMasking } from './tauri';

const masking: ConnectionMasking = {
  rules: [
    { table: 'public.users', column: 'email', mode: 'partial' },
    { table: '', column: 'ssn', mode: 'hash' },
  ],
  mask_detected_columns: false,
};

describe('findRule', () => {
  it('matches the table without its schema, and any table for free queries', () => {
    expect(findRule(masking, 'users', 'EMAIL')?.mode).toBe('partial');
    expect(findRule(masking, 'orders', 'email')).toBeUndefined();
    expect(findRule(masking, undefined, 'email')?.mode).toBe('partial');
    expect(findRule(masking, 'orders', 'ssn')?.mode).toBe('hash');
  });
});

describe('withRule / withoutRule', () => {
  it('replaces the rule of the same column and table', () => {
    const next = withRule(masking, { table: 'public.users', column: 'email', mode: 'hidden' });
    expect(next.rules).toHaveLength(2);
    expect(findRule(next, 'users', 'email')?.mode).toBe('hidden');
    expect(withoutRule(next, next.rules[1]).rules).toHaveLength(1);
  });
});

describe('removesMasking', () => {
  it('flags removals, weaker modes and detection switched off, not additions', () => {
    expect(removesMasking(masking, withoutRule(masking, masking.rules[0]))).toBe(true);
    expect(
      removesMasking(masking, withRule(masking, { table: '', column: 'ssn', mode: 'partial' }))
    ).toBe(true);
    expect(removesMasking({ ...EMPTY_MASKING, mask_detected_columns: true }, EMPTY_MASKING)).toBe(
      true
    );
    expect(
      removesMasking(masking, withRule(masking, { table: '', column: 'phone', mode: 'hidden' }))
    ).toBe(false);
  });
});

describe('normalizeMasking', () => {
  it('trims names and drops rules without a column', () => {
    const next = normalizeMasking({
      rules: [
        { table: ' users ', column: ' email ', mode: 'hidden' },
        { table: 'users', column: '  ', mode: 'hidden' },
      ],
      mask_detected_columns: true,
    });
    expect(next.rules).toEqual([{ table: 'users', column: 'email', mode: 'hidden' }]);
    expect(next.mask_detected_columns).toBe(true);
  });
});
