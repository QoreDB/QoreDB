// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import { CHANGELOG, type ChangelogEntry } from '@/data/changelog';
import { setWhatsNewOpen } from '@/lib/stores/modalStore';
import { APP_VERSION } from '@/lib/version';

const LAST_SEEN_KEY = 'qoredb_last_seen_version';

function compareVersions(a: string, b: string): number {
  const pa = a.split('.').map(n => Number.parseInt(n, 10) || 0);
  const pb = b.split('.').map(n => Number.parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const diff = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (diff !== 0) return diff > 0 ? 1 : -1;
  }
  return 0;
}

export function getChangelogFor(version: string): ChangelogEntry | null {
  return CHANGELOG.find(e => e.version === version) ?? null;
}

export function markVersionSeen(version: string): void {
  try {
    localStorage.setItem(LAST_SEEN_KEY, version);
  } catch {
    // ignore
  }
}

function readLastSeen(): string | null {
  try {
    return localStorage.getItem(LAST_SEEN_KEY);
  } catch {
    return null;
  }
}

/**
 * On mount, opens the What's New modal if the user is on a newer version
 * than what they last saw AND a changelog entry exists for the current version.
 *
 * First-ever launch (no stored version): silently records the current version
 * without showing the modal — the modal is for *upgrades*, not onboarding.
 */
export function useWhatsNew(): void {
  useEffect(() => {
    if (!AnalyticsService.isOnboardingCompleted()) return;
    const lastSeen = readLastSeen();
    if (!lastSeen) {
      markVersionSeen(APP_VERSION);
      return;
    }
    if (compareVersions(APP_VERSION, lastSeen) <= 0) return;
    if (!getChangelogFor(APP_VERSION)) {
      markVersionSeen(APP_VERSION);
      return;
    }
    setWhatsNewOpen(true);
  }, []);
}
