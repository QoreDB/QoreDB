// SPDX-License-Identifier: Apache-2.0

import type { LicenseTier } from '@/lib/license';

interface LicenseBadgeProps {
  tier?: LicenseTier;
  className?: string;
}

const TIER_LABELS: Record<LicenseTier, string> = {
  core: 'Core',
  pro: 'Pro',
  team: 'Team',
  enterprise: 'Enterprise',
};

/**
 * Discreet badge showing the license tier.
 * Only renders for non-Core tiers by default.
 */
export function LicenseBadge({ tier, className }: LicenseBadgeProps) {
  if (!tier || tier === 'core') return null;

  return (
    <span
      className={`inline-flex items-center rounded-md bg-accent/10 px-1.5 py-0.5 text-xs font-medium text-accent ${className ?? ''}`}
    >
      {TIER_LABELS[tier]}
    </span>
  );
}
