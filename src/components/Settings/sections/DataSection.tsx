// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
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
import type { CacheConfig, CacheStats, SafetyPolicy, TimeTravelConfig } from '@/lib/tauri';
import {
  clearQueryCache,
  getCacheConfig,
  getCacheStats,
  getTimeTravelConfig,
  setCacheConfig,
  updateTimeTravelConfig,
} from '@/lib/tauri';
import { useLicense } from '@/providers/LicenseProvider';
import { useWorkspace } from '@/providers/WorkspaceProvider';
import { ConfigBackupCard } from '../ConfigBackupCard';
import { ProjectTransferCard } from '../ProjectTransferCard';
import { SettingsCard } from '../SettingsCard';
import { ShareProviderCard } from '../ShareProviderCard';
import { BackupToolsCard } from './BackupToolsCard';

interface DataSectionProps {
  policy: SafetyPolicy | null;
  onApplyPolicy: (policy: SafetyPolicy) => Promise<void>;
  searchQuery?: string;
}

const DEFAULTS = {
  storeHistory: true,
  storeErrorLogs: true,
};

export function DataSection({ policy, onApplyPolicy, searchQuery }: DataSectionProps) {
  const { t } = useTranslation();
  const { projectId } = useWorkspace();
  const [diagnostics, setDiagnostics] = useState<DiagnosticsSettings>(getDiagnosticsSettings());

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
    diagnostics.storeErrorLogs !== DEFAULTS.storeErrorLogs;

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
        </div>
      </SettingsCard>

      <ConfigBackupCard
        policy={policy}
        onApplyDiagnostics={updateDiagnostics}
        onApplyPolicy={onApplyPolicy}
      />

      <ShareProviderCard searchQuery={searchQuery} />

      <BackupToolsCard searchQuery={searchQuery} />

      <ProjectTransferCard projectId={projectId} />

      <QueryCacheCard searchQuery={searchQuery} />

      <TimeTravelSettingsCard searchQuery={searchQuery} />
    </>
  );
}

const CACHE_DEFAULTS: CacheConfig = { enabled: true, ttlSecs: 60, maxEntries: 100 };

function QueryCacheCard({ searchQuery }: { searchQuery?: string }) {
  const { t } = useTranslation();
  const [config, setConfig] = useState<CacheConfig>(CACHE_DEFAULTS);
  const [stats, setStats] = useState<CacheStats | null>(null);
  const [loaded, setLoaded] = useState(false);

  const refreshStats = () => {
    getCacheStats()
      .then(setStats)
      .catch(() => {});
  };

  useEffect(() => {
    getCacheConfig()
      .then(c => {
        setConfig(c);
        setLoaded(true);
      })
      .catch(() => {});
    refreshStats();
  }, []);

  function update(patch: Partial<CacheConfig>) {
    const next = { ...config, ...patch };
    setConfig(next);
    setCacheConfig(next).catch(() => {});
  }

  async function clear() {
    await clearQueryCache().catch(() => {});
    refreshStats();
  }

  const isModified =
    loaded &&
    (config.enabled !== CACHE_DEFAULTS.enabled ||
      config.ttlSecs !== CACHE_DEFAULTS.ttlSecs ||
      config.maxEntries !== CACHE_DEFAULTS.maxEntries);

  const total = stats ? stats.hits + stats.misses : 0;
  const hitRate = total > 0 ? Math.round(((stats?.hits ?? 0) / total) * 100) : 0;

  return (
    <SettingsCard
      id="query-cache"
      title={t('cache.title')}
      description={t('cache.description')}
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
            <span className="font-medium text-foreground">{t('cache.enabledLabel')}</span>
            <span className="block text-xs text-muted-foreground mt-0.5">
              {t('cache.enabledDescription')}
            </span>
          </span>
        </Label>

        <div className="flex items-center gap-3">
          <Label className="w-40 text-sm text-foreground">{t('cache.ttl')}</Label>
          <Input
            type="number"
            min={5}
            max={3600}
            value={config.ttlSecs}
            disabled={!config.enabled}
            onChange={e => update({ ttlSecs: Math.max(5, Number(e.target.value) || 60) })}
            className="w-28"
          />
        </div>

        <div className="flex items-center gap-3">
          <Label className="w-40 text-sm text-foreground">{t('cache.maxEntries')}</Label>
          <Input
            type="number"
            min={10}
            max={1000}
            value={config.maxEntries}
            disabled={!config.enabled}
            onChange={e => update({ maxEntries: Math.max(10, Number(e.target.value) || 100) })}
            className="w-28"
          />
        </div>

        <div className="flex items-center justify-between border-t border-border/50 pt-3">
          <span className="text-xs text-muted-foreground">
            {t('cache.stats', { entries: stats?.entries ?? 0, hitRate })}
          </span>
          <Button variant="outline" size="sm" onClick={clear}>
            {t('cache.clear')}
          </Button>
        </div>
      </div>
    </SettingsCard>
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

export function TimeTravelSettingsCard({ searchQuery }: { searchQuery?: string }) {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();
  const [config, setConfig] = useState<TimeTravelConfig>(TT_DEFAULTS);
  const [excludedTablesInput, setExcludedTablesInput] = useState('');
  const [savedConfig, setSavedConfig] = useState<TimeTravelConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [failed, setFailed] = useState(false);
  const [loadAttempt, setLoadAttempt] = useState(0);
  const featureEnabled = isFeatureEnabled('data_time_travel');

  // biome-ignore lint/correctness/useExhaustiveDependencies: loadAttempt explicitly retries a failed settings read.
  useEffect(() => {
    if (!featureEnabled) return;
    let cancelled = false;
    setLoading(true);
    setFailed(false);
    getTimeTravelConfig()
      .then(res => {
        if (cancelled) return;
        if (!res.success) throw new Error('Time-travel settings unavailable');
        setConfig(res.config);
        setExcludedTablesInput(res.config.excluded_tables.join(', '));
        setSavedConfig(res.config);
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [featureEnabled, loadAttempt]);

  function update(patch: Partial<TimeTravelConfig>) {
    setConfig(current => ({ ...current, ...patch }));
  }

  async function save() {
    if (!savedConfig || loading || saving) return;
    setSaving(true);
    setFailed(false);
    try {
      const res = await updateTimeTravelConfig(config);
      if (!res.success) throw new Error('Time-travel settings not saved');
      setConfig(res.config);
      setExcludedTablesInput(res.config.excluded_tables.join(', '));
      setSavedConfig(res.config);
    } catch {
      setFailed(true);
    } finally {
      setSaving(false);
    }
  }

  const isModified =
    !!savedConfig &&
    (config.enabled !== TT_DEFAULTS.enabled ||
      config.retention_days !== TT_DEFAULTS.retention_days ||
      config.max_entries !== TT_DEFAULTS.max_entries ||
      config.production_only !== TT_DEFAULTS.production_only ||
      config.excluded_tables.length > 0);

  if (!featureEnabled) return null;

  return (
    <SettingsCard
      id="time-travel"
      title={t('timeTravel.settings.title')}
      description={t('timeTravel.settings.enabledDescription')}
      isModified={isModified}
      searchQuery={searchQuery}
    >
      <form
        onSubmit={event => {
          event.preventDefault();
          void save();
        }}
      >
        <fieldset disabled={!savedConfig || loading || saving} className="space-y-4">
          <Label className="flex items-start gap-2.5 text-sm cursor-pointer">
            <Checkbox
              checked={config.enabled}
              onCheckedChange={checked => update({ enabled: !!checked })}
              className="mt-0.5"
            />
            <span>
              <span className="font-medium text-foreground">
                {t('timeTravel.settings.enabled')}
              </span>
              <span className="block text-xs text-muted-foreground mt-0.5">
                {t('timeTravel.settings.enabledDescription')}
              </span>
            </span>
          </Label>

          <p className="text-xs text-muted-foreground bg-muted/50 rounded-md px-3 py-2">
            {t('timeTravel.settings.dataWarning')}
          </p>

          <div className="space-y-1">
            <Label htmlFor="time-travel-retention" className="text-sm font-medium text-foreground">
              {t('timeTravel.settings.retentionDays')}
            </Label>
            <p className="text-xs text-muted-foreground">
              {t('timeTravel.settings.retentionDaysDescription')}
            </p>
            <Input
              id="time-travel-retention"
              type="number"
              min={0}
              max={365}
              value={config.retention_days}
              onChange={e => update({ retention_days: parseInt(e.target.value, 10) || 0 })}
              className="w-32 h-8 text-sm"
            />
          </div>

          <div className="space-y-1">
            <Label
              htmlFor="time-travel-max-entries"
              className="text-sm font-medium text-foreground"
            >
              {t('timeTravel.settings.maxEntries')}
            </Label>
            <p className="text-xs text-muted-foreground">
              {t('timeTravel.settings.maxEntriesDescription')}
            </p>
            <Input
              id="time-travel-max-entries"
              type="number"
              min={1000}
              max={500000}
              step={1000}
              value={config.max_entries}
              onChange={e => update({ max_entries: parseInt(e.target.value, 10) || 50000 })}
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
            <Label htmlFor="time-travel-excluded" className="text-sm font-medium text-foreground">
              {t('timeTravel.settings.excludedTables')}
            </Label>
            <p className="text-xs text-muted-foreground">
              {t('timeTravel.settings.excludedTablesDescription')}
            </p>
            <Input
              id="time-travel-excluded"
              value={excludedTablesInput}
              onChange={e => {
                setExcludedTablesInput(e.target.value);
                update({
                  excluded_tables: e.target.value
                    .split(',')
                    .map(s => s.trim())
                    .filter(Boolean),
                });
              }}
              placeholder="migrations, sessions, schema_history"
              className="h-8 text-sm"
            />
          </div>
          <Button
            type="submit"
            size="sm"
            disabled={!failed && JSON.stringify(config) === JSON.stringify(savedConfig)}
          >
            {t('common.save')}
          </Button>
        </fieldset>
      </form>
      {failed && (
        <div role="alert" className="mt-3 text-sm text-destructive">
          <p>{t('timeTravel.requestFailed')}</p>
          {!savedConfig && (
            <Button variant="outline" size="sm" onClick={() => setLoadAttempt(value => value + 1)}>
              {t('timeTravel.retry')}
            </Button>
          )}
        </div>
      )}
    </SettingsCard>
  );
}
