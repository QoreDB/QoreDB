// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import type { Row } from '@/lib/tauri';
import { estimatePayloadBytes, TAB_PAYLOAD_BUDGET_BYTES } from './payloadSize';

const row = (...values: Row['values']): Row => ({ values });

describe('estimatePayloadBytes', () => {
  it('is zero for no rows', () => {
    expect(estimatePayloadBytes([])).toBe(0);
  });

  it('charges text by its length', () => {
    const short = estimatePayloadBytes([row('a')]);
    const long = estimatePayloadBytes([row('a'.repeat(1000))]);
    expect(long - short).toBe(999 * 2);
  });

  it('grows with the number of rows', () => {
    const one = estimatePayloadBytes([row('x', 1, true)]);
    const three = estimatePayloadBytes([row('x', 1, true), row('x', 1, true), row('x', 1, true)]);
    expect(three).toBe(one * 3);
  });

  // MongoDB projects a whole document into a single cell: charging it a scalar's
  // worth would make the heaviest tables look like the lightest.
  it('walks into nested documents', () => {
    const flat = estimatePayloadBytes([row({ a: 'x' })]);
    const nested = estimatePayloadBytes([row({ a: { b: { c: 'x'.repeat(500) } } })]);
    expect(nested).toBeGreaterThan(flat + 900);
  });

  it('walks into arrays', () => {
    const empty = estimatePayloadBytes([row([])]);
    const filled = estimatePayloadBytes([row(['x'.repeat(100), 'y'.repeat(100)])]);
    expect(filled).toBeGreaterThan(empty + 400);
  });

  it('stops at a bounded depth rather than exhausting the stack', () => {
    let deep: Record<string, unknown> = { leaf: 'x' };
    for (let i = 0; i < 5000; i++) deep = { next: deep };
    expect(() => estimatePayloadBytes([row(deep)])).not.toThrow();
  });

  it('treats null and undefined cells as overhead only', () => {
    expect(estimatePayloadBytes([row(null)])).toBe(estimatePayloadBytes([row(1)]));
  });

  // A page of multi-megabyte cells must trip the budget long before a page count
  // that a person could reach by scrolling.
  it('trips the budget on a handful of heavy pages', () => {
    const heavyPage = Array.from({ length: 100 }, () => row('x'.repeat(64 * 1024)));
    const perPage = estimatePayloadBytes(heavyPage);
    expect(Math.ceil(TAB_PAYLOAD_BUDGET_BYTES / perPage)).toBeLessThan(20);
  });
});
