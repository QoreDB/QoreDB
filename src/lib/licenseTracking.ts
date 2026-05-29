// SPDX-License-Identifier: Apache-2.0

import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import type { ProFeature } from '@/lib/license';

const DISMISSED_PROMPTS_KEY = 'qoredb_dismissed_upgrade_prompts';

export type ProEvent =
  | 'pro_feature_seen'
  | 'pro_feature_blocked'
  | 'pro_upgrade_prompt_seen'
  | 'pro_upgrade_cta_clicked'
  | 'pro_upgrade_learn_more_clicked'
  | 'pro_upgrade_dismissed'
  | 'pro_discovery_opened'
  | 'pro_discovery_feature_clicked';

export interface ProEventProps {
  feature?: ProFeature;
  source?: string;
}

export function trackProEvent(event: ProEvent, props?: ProEventProps): void {
  AnalyticsService.capture(event, props);
}

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
  trackProEvent('pro_upgrade_dismissed', { feature });
}

export function resetDismissedPrompts(): void {
  try {
    localStorage.removeItem(DISMISSED_PROMPTS_KEY);
  } catch {
    // ignore
  }
}
