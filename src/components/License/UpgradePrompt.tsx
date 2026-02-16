import { useTranslation } from 'react-i18next';
import type { ProFeature } from '@/lib/license';
import { featureRequiredTier } from '@/lib/license';
import { LicenseBadge } from './LicenseBadge';

interface UpgradePromptProps {
  feature: ProFeature;
  className?: string;
}

/**
 * Contextual upgrade prompt shown when a gated feature is accessed.
 * Follows the Design DNA: no blocking modal, no flashy animation.
 */
export function UpgradePrompt({ feature, className }: UpgradePromptProps) {
  const { t } = useTranslation();
  const requiredTier = featureRequiredTier(feature);

  return (
    <div
      className={`flex flex-col items-center justify-center gap-3 rounded-lg border border-dashed p-6 text-center ${className ?? ''}`}
      style={{ borderColor: 'rgba(107, 92, 255, 0.3)' }}
    >
      <LicenseBadge tier={requiredTier} />
      <p className="text-sm text-[var(--color-text-secondary)]">
        {t(`license.features.${feature}`, { defaultValue: t('license.upgrade.description') })}
      </p>
      <p className="text-xs text-[var(--color-text-tertiary)]">{t('license.upgrade.hint')}</p>
    </div>
  );
}
