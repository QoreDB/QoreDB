// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import { Checkbox } from '@/components/ui/checkbox';
import {
  type DiagnosticsSettings,
  getDiagnosticsSettings,
  setDiagnosticsSettings,
} from '@/lib/diagnosticsSettings';
import { clearErrorLogs } from '@/lib/errorLog';
import { clearHistory } from '@/lib/history';
import type { SafetyPolicy } from '@/lib/tauri';
import { ConfigBackupCard } from '../ConfigBackupCard';
import { ProjectTransferCard } from '../ProjectTransferCard';
import { SettingsCard } from '../SettingsCard';

interface DataSectionProps {
  policy: SafetyPolicy | null;
  onApplyPolicy: (policy: SafetyPolicy) => Promise<void>;
  searchQuery?: string;
}

const DEFAULT_PROJECT_ID = 'default';

// Default values for detecting modifications
const DEFAULTS = {
  storeHistory: true,
  storeErrorLogs: true,
  analyticsEnabled: true,
};

export function DataSection({ policy, onApplyPolicy, searchQuery }: DataSectionProps) {
  const { t } = useTranslation();
  const [diagnostics, setDiagnostics] = useState<DiagnosticsSettings>(getDiagnosticsSettings());
  const [analyticsEnabled, setAnalyticsEnabled] = useState<boolean>(
    AnalyticsService.isAnalyticsEnabled()
  );

  function updateDiagnostics(next: DiagnosticsSettings) {
    setDiagnostics(next);
    setDiagnosticsSettings(next);
    if (!next.storeHistory) {
      clearHistory();
    }
    if (!next.storeErrorLogs) {
      clearErrorLogs();
    }
  }

  const isDiagnosticsModified =
    diagnostics.storeHistory !== DEFAULTS.storeHistory ||
    diagnostics.storeErrorLogs !== DEFAULTS.storeErrorLogs ||
    analyticsEnabled !== DEFAULTS.analyticsEnabled;

  return (
    <>
      <SettingsCard
        id="diagnostics"
        title={t('settings.diagnostics')}
        description={t('settings.diagnosticsDescription')}
        isModified={isDiagnosticsModified}
        searchQuery={searchQuery}
      >
        <div className="space-y-3">
          <label className="flex items-start gap-2.5 text-sm cursor-pointer">
            <Checkbox
              checked={diagnostics.storeHistory}
              onCheckedChange={checked =>
                updateDiagnostics({
                  ...diagnostics,
                  storeHistory: !!checked,
                })
              }
              className="mt-0.5"
            />
            <span>
              <span className="font-medium text-foreground">{t('settings.storeHistory')}</span>
              <span className="block text-xs text-muted-foreground mt-0.5">
                {t('settings.storeHistoryDescription')}
              </span>
            </span>
          </label>

          <label className="flex items-start gap-2.5 text-sm cursor-pointer">
            <Checkbox
              checked={diagnostics.storeErrorLogs}
              onCheckedChange={checked =>
                updateDiagnostics({
                  ...diagnostics,
                  storeErrorLogs: !!checked,
                })
              }
              className="mt-0.5"
            />
            <span>
              <span className="font-medium text-foreground">{t('settings.storeErrorLogs')}</span>
              <span className="block text-xs text-muted-foreground mt-0.5">
                {t('settings.storeErrorLogsDescription')}
              </span>
            </span>
          </label>

          <label className="flex items-start gap-2.5 text-sm cursor-pointer">
            <Checkbox
              checked={analyticsEnabled}
              onCheckedChange={checked => {
                const enabled = !!checked;
                setAnalyticsEnabled(enabled);
                AnalyticsService.setAnalyticsEnabled(enabled);
              }}
              className="mt-0.5"
            />
            <span>
              <span className="font-medium text-foreground">{t('settings.analyticsEnabled')}</span>
              <span className="block text-xs text-muted-foreground mt-0.5">
                {t('settings.analyticsEnabledDescription')}
              </span>
            </span>
          </label>
        </div>
      </SettingsCard>

      <ConfigBackupCard
        policy={policy}
        onApplyDiagnostics={updateDiagnostics}
        onApplyPolicy={onApplyPolicy}
        onApplyAnalyticsEnabled={(enabled: boolean) => {
          setAnalyticsEnabled(enabled);
          AnalyticsService.setAnalyticsEnabled(enabled);
        }}
      />

      <ProjectTransferCard projectId={DEFAULT_PROJECT_ID} />
    </>
  );
}
