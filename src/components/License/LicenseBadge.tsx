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
      className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-xs font-medium ${className ?? ''}`}
      style={{ color: '#6B5CFF', backgroundColor: 'rgba(107, 92, 255, 0.1)' }}
    >
      {TIER_LABELS[tier]}
    </span>
  );
}
