// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import type { QueryResultEntry } from './QueryPanelResults';
import { describeQueryStatus } from './queryAnnouncement';

const entry = (overrides: Partial<QueryResultEntry>): QueryResultEntry => ({
  id: 'q1',
  kind: 'query',
  query: 'SELECT 1',
  executedAt: 0,
  ...overrides,
});

describe('describeQueryStatus', () => {
  it('announces nothing before the first run', () => {
    expect(describeQueryStatus(false, [], null)).toBeNull();
  });

  it('announces a running query before anything else', () => {
    expect(describeQueryStatus(true, [entry({ error: 'x' })], 'boom')).toEqual({
      key: 'a11y.query.running',
    });
  });

  it('announces the latest result only', () => {
    const results = [entry({ error: 'old failure' }), entry({ rowCount: 42, totalTimeMs: 12.6 })];
    expect(describeQueryStatus(false, results, null)).toEqual({
      key: 'a11y.query.rows',
      count: 42,
      ms: 13,
    });
  });

  it('distinguishes failures, timeouts and statements without rows', () => {
    expect(describeQueryStatus(false, [], 'panel error')).toEqual({ key: 'a11y.query.failed' });
    expect(describeQueryStatus(false, [entry({ error: 'syntax' })], null)).toEqual({
      key: 'a11y.query.failed',
    });
    expect(describeQueryStatus(false, [entry({ timedOut: true, error: 't' })], null)).toEqual({
      key: 'a11y.query.timedOut',
    });
    expect(describeQueryStatus(false, [entry({ executionTimeMs: 3 })], null)).toEqual({
      key: 'a11y.query.done',
      ms: 3,
    });
  });
});
