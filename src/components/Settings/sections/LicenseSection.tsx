// SPDX-License-Identifier: Apache-2.0

import { FlaskConical, Lock, Unlock } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { LicenseActivation } from '@/components/License/LicenseActivation';
import { Button } from '@/components/ui/button';
import type { LicenseTier, ProFeature } from '@/lib/license';
import { useLicense } from '@/providers/LicenseProvider';
import { SettingsCard } from '../SettingsCard';

interface LicenseSectionProps {
  searchQuery?: string;
}

const PRO_FEATURES: ProFeature[] = [
  'sandbox',
  'visual_diff',
  'profiling',
  'custom_safety_rules',
  'audit_advanced',
  'ai',
  'export_xlsx',
  'export_parquet',
  'query_library_advanced',
  'virtual_relations_auto_suggest',
];

const DEV_TIERS: LicenseTier[] = ['core', 'pro', 'team', 'enterprise'];

export function LicenseSection({ searchQuery }: LicenseSectionProps) {
  const { t } = useTranslation();
  const { tier, isFeatureEnabled, devSetTier } = useLicense();

  return (
    <div className="space-y-6">
      <SettingsCard
        title={t('settings.license.title')}
        description={t('settings.license.description')}
        searchQuery={searchQuery}
      >
        <LicenseActivation />
      </SettingsCard>

      <SettingsCard
        title={t('settings.license.featuresTitle')}
        description={t('settings.license.featuresDescription')}
        searchQuery={searchQuery}
      >
        <ul className="space-y-1.5">
          {PRO_FEATURES.map(feature => {
            const enabled = isFeatureEnabled(feature);
            return (
              <li key={feature} className="flex items-center gap-2 text-sm">
                {enabled ? (
                  <Unlock size={14} className="text-green-500 shrink-0" />
                ) : (
                  <Lock size={14} className="text-muted-foreground shrink-0" />
                )}
                <span className={enabled ? 'text-(--color-text-primary)' : 'text-muted-foreground'}>
                  {t(`settings.license.featureNames.${feature}`)}
                </span>
                {!enabled && (
                  <span className="ml-auto text-xs font-medium" style={{ color: '#6B5CFF' }}>
                    Pro
                  </span>
                )}
              </li>
            );
          })}
        </ul>
      </SettingsCard>

      {/* Dev-only tier override — stripped from production builds by Vite */}
      {import.meta.env.DEV && (
        <SettingsCard
          title={t('settings.license.devOverrideTitle')}
          description={t('settings.license.devOverrideDescription')}
          searchQuery={searchQuery}
        >
          <div className="flex items-center gap-2">
            <FlaskConical size={14} className="text-warning shrink-0" />
            <span className="text-xs text-warning font-medium">
              {t('settings.license.devOnly')}
            </span>
          </div>
          <div className="flex items-center gap-1.5 mt-3">
            {DEV_TIERS.map(devTier => (
              <Button
                key={devTier}
                variant={tier === devTier ? 'default' : 'outline'}
                size="sm"
                className="h-7 text-xs capitalize"
                onClick={() => devSetTier(devTier === 'core' ? null : devTier)}
              >
                {devTier}
              </Button>
            ))}
          </div>
        </SettingsCard>
      )}
    </div>
  );
}
