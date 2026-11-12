// SPDX-License-Identifier: Apache-2.0

import type { QueryResultEntry } from './QueryPanelResults';

export type QueryAnnouncement =
  | { key: 'a11y.query.running' }
  | { key: 'a11y.query.failed' }
  | { key: 'a11y.query.timedOut' }
  | { key: 'a11y.query.rows'; count: number; ms: number }
  | { key: 'a11y.query.done'; ms: number };

export function describeQueryStatus(
  loading: boolean,
  results: QueryResultEntry[],
  panelError: string | null
): QueryAnnouncement | null {
  if (loading) return { key: 'a11y.query.running' };
  if (panelError) return { key: 'a11y.query.failed' };
  const latest = results[results.length - 1];
  if (!latest) return null;
  if (latest.timedOut) return { key: 'a11y.query.timedOut' };
  if (latest.error) return { key: 'a11y.query.failed' };
  const ms = Math.round(latest.totalTimeMs ?? latest.executionTimeMs ?? 0);
  const count = latest.rowCount ?? latest.result?.rows.length;
  return count == null ? { key: 'a11y.query.done', ms } : { key: 'a11y.query.rows', count, ms };
}
