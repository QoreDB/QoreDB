// SPDX-License-Identifier: Apache-2.0

import { Crown } from 'lucide-react';
import { useTranslation } from 'react-i18next';

interface FounderBadgeProps {
  className?: string;
}

/**
 * Discreet badge for early Pro adopters (first batch of buyers).
 * Driven by the `is_founder` flag in the signed license payload.
 */
export function FounderBadge({ className }: FounderBadgeProps) {
  const { t } = useTranslation();
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-xs font-medium ${className ?? ''}`}
      style={{ color: '#D4A12C', backgroundColor: 'rgba(212, 161, 44, 0.12)' }}
      title={t('license.founderTooltip', 'Thank you for being an early supporter.')}
    >
      <Crown size={10} aria-hidden />
      {t('license.founderBadge', 'Founder')}
    </span>
  );
}
