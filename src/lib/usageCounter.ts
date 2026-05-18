// SPDX-License-Identifier: Apache-2.0

const COUNT_KEY = 'qoredb_query_count';
const LAST_BANNER_KEY = 'qoredb_usage_banner_last_shown';
const DISMISSED_THRESHOLDS_KEY = 'qoredb_usage_dismissed_thresholds';

export const USAGE_THRESHOLDS = [500, 1000, 5000, 10000] as const;
export type UsageThreshold = (typeof USAGE_THRESHOLDS)[number];

const MIN_INTERVAL_MS = 30 * 24 * 60 * 60 * 1000;

function readNumber(key: string): number {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return 0;
    const parsed = Number.parseInt(raw, 10);
    return Number.isFinite(parsed) ? parsed : 0;
  } catch {
    return 0;
  }
}

function writeNumber(key: string, value: number): void {
  try {
    localStorage.setItem(key, String(value));
  } catch {
    // ignore
  }
}

function readDismissed(): Set<number> {
  try {
    const raw = localStorage.getItem(DISMISSED_THRESHOLDS_KEY);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) return new Set(parsed);
  } catch {
    // ignore
  }
  return new Set();
}

function writeDismissed(set: Set<number>): void {
  try {
    localStorage.setItem(DISMISSED_THRESHOLDS_KEY, JSON.stringify([...set]));
  } catch {
    // ignore
  }
}

export function getQueryCount(): number {
  return readNumber(COUNT_KEY);
}

export function incrementQueryCount(): number {
  const next = readNumber(COUNT_KEY) + 1;
  writeNumber(COUNT_KEY, next);
  return next;
}

/**
 * Returns a threshold that the user has *just crossed* and that hasn't been
 * shown recently. Returns null if no banner should appear.
 *
 * Rules:
 * - Only the highest unseen threshold ≤ current count is returned (so we don't backfill).
 * - A threshold can only be shown once (dismissed list).
 * - Minimum 30 days between banners regardless of threshold.
 */
export function getUsageBannerThreshold(count: number): UsageThreshold | null {
  const dismissed = readDismissed();
  const lastShown = readNumber(LAST_BANNER_KEY);
  if (lastShown && Date.now() - lastShown < MIN_INTERVAL_MS) return null;

  let candidate: UsageThreshold | null = null;
  for (const threshold of USAGE_THRESHOLDS) {
    if (count >= threshold && !dismissed.has(threshold)) {
      candidate = threshold;
    }
  }
  return candidate;
}

export function markUsageBannerShown(threshold: UsageThreshold): void {
  const dismissed = readDismissed();
  dismissed.add(threshold);
  writeDismissed(dismissed);
  writeNumber(LAST_BANNER_KEY, Date.now());
}
