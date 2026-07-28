// SPDX-License-Identifier: Apache-2.0

import type { ProFeature } from '@/lib/license';

const DISMISSED_PROMPTS_KEY = 'qoredb_dismissed_upgrade_prompts';

function readDismissedSet(): Set<string> {
  try {
    const raw = localStorage.getItem(DISMISSED_PROMPTS_KEY);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) return new Set(parsed);
  } catch {
    // ignore
  }
  return new Set();
}

function writeDismissedSet(set: Set<string>): void {
  try {
    localStorage.setItem(DISMISSED_PROMPTS_KEY, JSON.stringify([...set]));
  } catch {
    // ignore
  }
}

export function isPromptDismissed(feature: ProFeature): boolean {
  return readDismissedSet().has(feature);
}

export function dismissPrompt(feature: ProFeature): void {
  const set = readDismissedSet();
  set.add(feature);
  writeDismissedSet(set);
}

export function resetDismissedPrompts(): void {
  try {
    localStorage.removeItem(DISMISSED_PROMPTS_KEY);
  } catch {
    // ignore
  }
}
