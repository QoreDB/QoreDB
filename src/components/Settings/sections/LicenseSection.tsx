// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from 'react-i18next';
import { SettingsCard } from '../SettingsCard';
import { LicenseActivation } from '@/components/License/LicenseActivation';
import { useLicense } from '@/providers/LicenseProvider';
import { Lock, Unlock } from 'lucide-react';
import type { ProFeature } from '@/lib/license';

interface LicenseSectionProps {
  searchQuery?: string;
}

const PRO_FEATURES: ProFeature[] = [
  'sandbox',
  'visual_diff',
  'er_diagram',
  'profiling',
  'custom_safety_rules',
  'audit_advanced',
  'ai',
  'export_xlsx',
  'export_parquet',
  'query_library_advanced',
  'virtual_relations_auto_suggest',
];

export function LicenseSection({ searchQuery }: LicenseSectionProps) {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();

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
    </div>
  );
}
