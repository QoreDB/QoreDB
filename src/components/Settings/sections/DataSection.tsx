// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  type DiagnosticsSettings,
  getDiagnosticsSettings,
  setDiagnosticsSettings,
} from '@/lib/diagnostics/diagnosticsSettings';
import { clearErrorLogs } from '@/lib/diagnostics/errorLog';
import { clearHistory } from '@/lib/query/history';
import type { SafetyPolicy, TimeTravelConfig } from '@/lib/tauri';
import { getTimeTravelConfig, updateTimeTravelConfig } from '@/lib/tauri';
import { useLicense } from '@/providers/LicenseProvider';
import { useWorkspace } from '@/providers/WorkspaceProvider';
import { ConfigBackupCard } from '../ConfigBackupCard';
import { ProjectTransferCard } from '../ProjectTransferCard';
import { SettingsCard } from '../SettingsCard';
import { ShareProviderCard } from '../ShareProviderCard';

interface DataSectionProps {
  policy: SafetyPolicy | null;
  onApplyPolicy: (policy: SafetyPolicy) => Promise<void>;
  searchQuery?: string;
}

const DEFAULTS = {
  storeHistory: true,
  storeErrorLogs: true,
  analyticsEnabled: true,
};

export function DataSection({ policy, onApplyPolicy, searchQuery }: DataSectionProps) {
  const { t } = useTranslation();
  const { projectId } = useWorkspace();
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
          <Label className="flex items-start gap-2.5 text-sm cursor-pointer">
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
          </Label>

          <Label className="flex items-start gap-2.5 text-sm cursor-pointer">
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
          </Label>

          <Label className="flex items-start gap-2.5 text-sm cursor-pointer">
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
          </Label>
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

      <ShareProviderCard searchQuery={searchQuery} />

      <ProjectTransferCard projectId={projectId} />

      <TimeTravelSettingsCard searchQuery={searchQuery} />
    </>
  );
}

const TT_DEFAULTS: TimeTravelConfig = {
  enabled: true,
  max_entries: 50_000,
  retention_days: 30,
  max_file_size_mb: 500,
  excluded_tables: [],
  production_only: false,
};

function TimeTravelSettingsCard({ searchQuery }: { searchQuery?: string }) {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();
  const [config, setConfig] = useState<TimeTravelConfig>(TT_DEFAULTS);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (!isFeatureEnabled('data_time_travel')) return;
    getTimeTravelConfig()
      .then(res => {
        if (res.success) {
          setConfig(res.config);
          setLoaded(true);
        }
      })
      .catch(() => {});
  }, [isFeatureEnabled]);

  function update(patch: Partial<TimeTravelConfig>) {
    const next = { ...config, ...patch };
    setConfig(next);
    updateTimeTravelConfig(next).catch(() => {});
  }

  const isModified =
    loaded &&
    (config.enabled !== TT_DEFAULTS.enabled ||
      config.retention_days !== TT_DEFAULTS.retention_days ||
      config.max_entries !== TT_DEFAULTS.max_entries ||
      config.production_only !== TT_DEFAULTS.production_only ||
      config.excluded_tables.length > 0);

  if (!isFeatureEnabled('data_time_travel')) return null;

  return (
    <SettingsCard
      id="time-travel"
      title={t('timeTravel.settings.title')}
      description={t('timeTravel.settings.enabledDescription')}
      isModified={isModified}
      searchQuery={searchQuery}
    >
      <div className="space-y-4">
        <Label className="flex items-start gap-2.5 text-sm cursor-pointer">
          <Checkbox
            checked={config.enabled}
            onCheckedChange={checked => update({ enabled: !!checked })}
            className="mt-0.5"
          />
          <span>
            <span className="font-medium text-foreground">{t('timeTravel.settings.enabled')}</span>
            <span className="block text-xs text-muted-foreground mt-0.5">
              {t('timeTravel.settings.enabledDescription')}
            </span>
          </span>
        </Label>

        <p className="text-xs text-muted-foreground bg-muted/50 rounded-md px-3 py-2">
          {t('timeTravel.settings.dataWarning')}
        </p>

        <div className="space-y-1">
          <Label className="text-sm font-medium text-foreground">
            {t('timeTravel.settings.retentionDays')}
          </Label>
          <p className="text-xs text-muted-foreground">
            {t('timeTravel.settings.retentionDaysDescription')}
          </p>
          <Input
            type="number"
            min={0}
            max={365}
            value={config.retention_days}
            onChange={e => update({ retention_days: parseInt(e.target.value) || 0 })}
            className="w-32 h-8 text-sm"
          />
        </div>

        <div className="space-y-1">
          <Label className="text-sm font-medium text-foreground">
            {t('timeTravel.settings.maxEntries')}
          </Label>
          <p className="text-xs text-muted-foreground">
            {t('timeTravel.settings.maxEntriesDescription')}
          </p>
          <Input
            type="number"
            min={1000}
            max={500000}
            step={1000}
            value={config.max_entries}
            onChange={e => update({ max_entries: parseInt(e.target.value) || 50000 })}
            className="w-32 h-8 text-sm"
          />
        </div>

        <Label className="flex items-start gap-2.5 text-sm cursor-pointer">
          <Checkbox
            checked={config.production_only}
            onCheckedChange={checked => update({ production_only: !!checked })}
            className="mt-0.5"
          />
          <span>
            <span className="font-medium text-foreground">
              {t('timeTravel.settings.productionOnly')}
            </span>
            <span className="block text-xs text-muted-foreground mt-0.5">
              {t('timeTravel.settings.productionOnlyDescription')}
            </span>
          </span>
        </Label>

        <div className="space-y-1">
          <Label className="text-sm font-medium text-foreground">
            {t('timeTravel.settings.excludedTables')}
          </Label>
          <p className="text-xs text-muted-foreground">
            {t('timeTravel.settings.excludedTablesDescription')}
          </p>
          <Input
            value={config.excluded_tables.join(', ')}
            onChange={e =>
              update({
                excluded_tables: e.target.value
                  .split(',')
                  .map(s => s.trim())
                  .filter(Boolean),
              })
            }
            placeholder="migrations, sessions, schema_history"
            className="h-8 text-sm"
          />
        </div>
      </div>
    </SettingsCard>
  );
}
