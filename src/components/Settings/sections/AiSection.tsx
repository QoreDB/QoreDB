// SPDX-License-Identifier: BUSL-1.1

import {
  AlertCircle,
  Check,
  ChevronDown,
  Cloud,
  Cpu,
  Download,
  Eye,
  EyeOff,
  Loader2,
  Play,
  Square,
  Trash2,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AiProviderSelector } from '@/components/AI/AiProviderSelector';
import { LicenseGate } from '@/components/License/LicenseGate';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import {
  AI_PROVIDERS,
  type AiProvider,
  type AiProviderInfo,
  aiCancelLocalRuntimeInstallation,
  aiDeleteApiKey,
  aiInstallLocalRuntime,
  aiSaveApiKey,
  aiStartLocalRuntime,
  aiStopLocalRuntime,
  type LocalInstallProgress,
} from '@/lib/ai';
import { listen } from '@/lib/transport';
import { cn } from '@/lib/utils';
import { useAiPreferences } from '@/providers/AiPreferencesProvider';
import { SettingsCard } from '../SettingsCard';

interface AiSectionProps {
  searchQuery?: string;
}

const INSTALL_PROGRESS_EVENT = 'ai-local-install-progress';

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toFixed(index >= 3 ? 1 : 0)} ${units[index]}`;
}

function ProviderStatusBadge({
  ready,
  local,
  installing,
}: {
  ready: boolean;
  local: boolean;
  installing?: boolean;
}) {
  const { t } = useTranslation();
  if (installing) {
    return (
      <Badge variant="outline" className="gap-1 border-accent/30 text-accent">
        <Loader2 size={11} className="animate-spin" />
        {t('ai.settings.installing')}
      </Badge>
    );
  }
  return ready ? (
    <Badge variant="outline" className="gap-1 border-success/30 text-success">
      <Check size={11} />
      {t('ai.settings.ready')}
    </Badge>
  ) : (
    <Badge variant="outline" className="gap-1 border-warning/30 text-warning">
      <AlertCircle size={11} />
      {t(local ? 'ai.settings.notInstalled' : 'ai.settings.notConfigured')}
    </Badge>
  );
}

function ApiKeyEditor({
  provider,
  hasKey,
  onChanged,
}: {
  provider: AiProvider;
  hasKey: boolean;
  onChanged: () => Promise<void>;
}) {
  const { t } = useTranslation();
  const [key, setKey] = useState('');
  const [showKey, setShowKey] = useState(false);
  const [busy, setBusy] = useState(false);

  const save = async () => {
    if (!key.trim()) return;
    setBusy(true);
    try {
      await aiSaveApiKey(provider, key.trim());
      setKey('');
      await onChanged();
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    setBusy(true);
    try {
      await aiDeleteApiKey(provider);
      await onChanged();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-2">
      <div className="flex gap-2">
        <div className="relative flex-1">
          <Input
            type={showKey ? 'text' : 'password'}
            value={key}
            onChange={event => setKey(event.target.value)}
            onKeyDown={event => event.key === 'Enter' && void save()}
            placeholder={hasKey ? '••••••••••••' : t('ai.settings.enterKey')}
            className="h-8 pr-8"
          />
          <Button
            type="button"
            variant="ghost"
            size="icon"
            onClick={() => setShowKey(value => !value)}
            className="absolute right-1 top-1/2 h-6 w-6 -translate-y-1/2"
          >
            {showKey ? <EyeOff size={14} /> : <Eye size={14} />}
          </Button>
        </div>
        <Button size="sm" className="h-8" disabled={!key.trim() || busy} onClick={save}>
          {busy ? <Loader2 size={14} className="animate-spin" /> : t('common.save')}
        </Button>
      </div>
      {hasKey && (
        <Button
          variant="ghost"
          size="sm"
          className="h-7 text-xs text-destructive hover:text-destructive"
          disabled={busy}
          onClick={remove}
        >
          <Trash2 size={12} className="mr-1" />
          {t('ai.settings.deleteKey')}
        </Button>
      )}
    </div>
  );
}

function ProviderRow({
  provider,
  expanded,
  onToggle,
}: {
  provider: AiProviderInfo;
  expanded: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const {
    preferredProvider,
    setPreferredProvider,
    preferredModels,
    setPreferredModel,
    preferredBaseUrls,
    setPreferredBaseUrl,
    providerStatuses,
    localRuntimeStatus,
    providerReady,
    refreshStatuses,
  } = useAiPreferences();
  const [runtimeBusy, setRuntimeBusy] = useState(false);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [installProgress, setInstallProgress] = useState<LocalInstallProgress | null>(null);
  const [endpointDraft, setEndpointDraft] = useState(preferredBaseUrls[provider.id] ?? '');
  const status = providerStatuses.find(item => item.provider === provider.id);
  const models = status?.models.length ? status.models : provider.models;
  const selectedModel = preferredModels[provider.id] ?? status?.default_model ?? models[0]?.id;
  const ready = providerReady[provider.id];
  const isManagedLocal = provider.kind === 'managed_local';

  useEffect(() => {
    if (localRuntimeStatus?.installation) {
      setInstallProgress(localRuntimeStatus.installation);
    }
  }, [localRuntimeStatus?.installation]);

  useEffect(() => {
    if (!isManagedLocal) return;
    let disposed = false;
    const unlisten = listen<LocalInstallProgress>(INSTALL_PROGRESS_EVENT, event => {
      if (!disposed) setInstallProgress(event.payload);
    });
    return () => {
      disposed = true;
      void unlisten.then(dispose => dispose());
    };
  }, [isManagedLocal]);

  const updateRuntime = async (start: boolean) => {
    setRuntimeBusy(true);
    setRuntimeError(null);
    try {
      if (start) await aiStartLocalRuntime();
      else await aiStopLocalRuntime();
      await refreshStatuses();
    } catch (error) {
      setRuntimeError(error instanceof Error ? error.message : String(error));
    } finally {
      setRuntimeBusy(false);
    }
  };

  const installRuntime = async () => {
    setRuntimeBusy(true);
    setRuntimeError(null);
    try {
      await aiInstallLocalRuntime();
      await refreshStatuses();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (!message.toLowerCase().includes('cancel')) setRuntimeError(message);
      await refreshStatuses();
    } finally {
      setRuntimeBusy(false);
    }
  };

  const cancelInstallation = async () => {
    try {
      await aiCancelLocalRuntimeInstallation();
    } catch (error) {
      setRuntimeError(error instanceof Error ? error.message : String(error));
    }
  };

  const isInstalling =
    localRuntimeStatus?.state === 'installing' ||
    (runtimeBusy && installProgress?.phase !== 'completed' && installProgress?.phase !== 'error');
  const progressPercent = installProgress?.total_bytes
    ? Math.min(100, (installProgress.downloaded_bytes / installProgress.total_bytes) * 100)
    : 0;

  return (
    <div className={cn('rounded-lg border transition-colors', expanded && 'border-accent/40')}>
      <button
        type="button"
        className="flex w-full items-center gap-3 p-3 text-left"
        onClick={onToggle}
        aria-expanded={expanded}
      >
        <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-muted">
          {provider.kind === 'cloud' ? <Cloud size={17} /> : <Cpu size={17} />}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium">{provider.label}</span>
            {provider.kind === 'managed_local' && (
              <Badge variant="secondary" className="text-[10px]">
                {t('ai.settings.recommended')}
              </Badge>
            )}
            {provider.kind === 'external_local' && (
              <Badge variant="secondary" className="text-[10px]">
                {t('ai.settings.external')}
              </Badge>
            )}
          </div>
          <p className="truncate text-xs text-muted-foreground">
            {selectedModel ? models.find(model => model.id === selectedModel)?.label : ''}
          </p>
        </div>
        <ProviderStatusBadge ready={ready} local={isManagedLocal} installing={isInstalling} />
        <ChevronDown size={15} className={cn('transition-transform', expanded && 'rotate-180')} />
      </button>

      {expanded && (
        <div className="space-y-4 border-t px-3 pb-4 pt-3">
          {isManagedLocal && (
            <div className="rounded-md bg-muted/40 p-3 text-xs text-muted-foreground">
              <p>{t('ai.settings.qoreLocalDescription')}</p>
              <p className="mt-1">
                {t('ai.settings.runtimeTarget', {
                  platform: localRuntimeStatus?.platform ?? '—',
                  architecture: localRuntimeStatus?.architecture ?? '—',
                })}
              </p>
              {(localRuntimeStatus?.state === 'not_installed' ||
                localRuntimeStatus?.state === 'error' ||
                isInstalling) && (
                <div className="mt-3 space-y-2">
                  <p>
                    {t('ai.settings.installDescription', {
                      size: formatBytes(localRuntimeStatus?.required_download_bytes ?? 0),
                    })}
                  </p>
                  {isInstalling && installProgress && (
                    <div className="space-y-1.5">
                      <div className="flex justify-between gap-3">
                        <span>
                          {t(`ai.settings.${installProgress.phase}`)} ·{' '}
                          {installProgress.artifact
                            ? t(`ai.settings.${installProgress.artifact}Artifact`)
                            : ''}
                        </span>
                        <span className="tabular-nums">
                          {t('ai.settings.downloadProgress', {
                            current: formatBytes(installProgress.downloaded_bytes),
                            total: formatBytes(installProgress.total_bytes),
                          })}
                        </span>
                      </div>
                      <div className="h-1.5 overflow-hidden rounded-full bg-muted">
                        <div
                          className="h-full rounded-full bg-accent transition-[width]"
                          style={{ width: `${progressPercent}%` }}
                        />
                      </div>
                    </div>
                  )}
                  <Button
                    size="sm"
                    variant="outline"
                    className="h-8"
                    disabled={runtimeBusy && !isInstalling}
                    onClick={() =>
                      isInstalling ? void cancelInstallation() : void installRuntime()
                    }
                  >
                    {isInstalling ? (
                      <Square size={12} className="mr-1" />
                    ) : runtimeBusy ? (
                      <Loader2 size={13} className="mr-1 animate-spin" />
                    ) : (
                      <Download size={13} className="mr-1" />
                    )}
                    {t(isInstalling ? 'ai.settings.cancelInstall' : 'ai.settings.installLocal')}
                  </Button>
                </div>
              )}
              {(runtimeError || localRuntimeStatus?.error) && (
                <p className="mt-2 text-destructive">{runtimeError ?? localRuntimeStatus?.error}</p>
              )}
              {(localRuntimeStatus?.state === 'ready' ||
                localRuntimeStatus?.state === 'running') && (
                <Button
                  size="sm"
                  variant="outline"
                  className="mt-3 h-8"
                  disabled={runtimeBusy}
                  onClick={() => void updateRuntime(localRuntimeStatus.state !== 'running')}
                >
                  {runtimeBusy ? (
                    <Loader2 size={13} className="mr-1 animate-spin" />
                  ) : localRuntimeStatus.state === 'running' ? (
                    <Square size={12} className="mr-1" />
                  ) : (
                    <Play size={13} className="mr-1" />
                  )}
                  {t(
                    localRuntimeStatus.state === 'running'
                      ? 'ai.settings.stopRuntime'
                      : 'ai.settings.startRuntime'
                  )}
                </Button>
              )}
            </div>
          )}

          {!isManagedLocal && (
            <>
              {provider.requiresKey && (
                <ApiKeyEditor
                  provider={provider.id}
                  hasKey={status?.has_key ?? false}
                  onChanged={refreshStatuses}
                />
              )}
              {provider.kind === 'external_local' && (
                <div className="space-y-1.5">
                  <label className="text-xs font-medium" htmlFor={`${provider.id}-endpoint`}>
                    {t('ai.settings.endpoint')}
                  </label>
                  <Input
                    id={`${provider.id}-endpoint`}
                    className="h-8 font-mono text-xs"
                    value={endpointDraft}
                    placeholder={provider.defaultBaseUrl}
                    onChange={event => setEndpointDraft(event.target.value)}
                    onBlur={() => setPreferredBaseUrl(provider.id, endpointDraft.trim())}
                    onKeyDown={event => event.key === 'Enter' && event.currentTarget.blur()}
                  />
                </div>
              )}
            </>
          )}

          <div className="flex flex-wrap items-end justify-between gap-3">
            <div className="min-w-56 space-y-1.5">
              <label className="text-xs font-medium" htmlFor={`${provider.id}-model`}>
                {t('ai.settings.defaultModel')}
              </label>
              <Select
                value={selectedModel}
                onValueChange={model => setPreferredModel(provider.id, model)}
              >
                <SelectTrigger id={`${provider.id}-model`} className="h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {models.map(model => (
                    <SelectItem key={model.id} value={model.id}>
                      {model.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <Button
              size="sm"
              variant={preferredProvider === provider.id ? 'secondary' : 'outline'}
              disabled={!ready || preferredProvider === provider.id}
              onClick={() => setPreferredProvider(provider.id)}
            >
              {preferredProvider === provider.id
                ? t('ai.settings.active')
                : t('ai.settings.useProvider')}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

export function AiSection({ searchQuery }: AiSectionProps) {
  const { t } = useTranslation();
  const {
    preferredProvider,
    setPreferredProvider,
    preferredModels,
    providerStatuses,
    providerReady,
    includeSampleRows,
    setIncludeSampleRows,
    allowSensitiveData,
    setAllowSensitiveData,
  } = useAiPreferences();
  const [expandedProvider, setExpandedProvider] = useState<AiProvider | null>(preferredProvider);
  const activeProvider = AI_PROVIDERS.find(provider => provider.id === preferredProvider);
  const activeStatus = providerStatuses.find(status => status.provider === preferredProvider);
  const activeModel =
    preferredModels[preferredProvider] ??
    activeStatus?.default_model ??
    activeProvider?.models[0]?.id;
  const isLocal = activeProvider?.kind !== 'cloud';

  return (
    <LicenseGate feature="ai">
      <div className="space-y-2">
        <SettingsCard
          title={t('ai.settings.activeTitle')}
          description={t('ai.settings.activeDescription')}
          searchQuery={searchQuery}
        >
          <div className="flex flex-wrap items-center gap-3 rounded-lg border bg-muted/20 p-4">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-background shadow-sm">
              {isLocal ? <Cpu size={19} /> : <Cloud size={19} />}
            </div>
            <div className="min-w-44 flex-1">
              <p className="text-sm font-medium">{activeProvider?.label}</p>
              <p className="text-xs text-muted-foreground">
                {activeStatus?.models.find(model => model.id === activeModel)?.label ?? activeModel}
              </p>
            </div>
            <AiProviderSelector
              provider={preferredProvider}
              onProviderChange={setPreferredProvider}
              providerReady={providerReady}
            />
          </div>
        </SettingsCard>

        <SettingsCard
          title={t('ai.settings.providersTitle')}
          description={t('ai.settings.providersDescription')}
          searchQuery={searchQuery}
        >
          <div className="space-y-2">
            {AI_PROVIDERS.map(provider => (
              <ProviderRow
                key={provider.id}
                provider={provider}
                expanded={expandedProvider === provider.id}
                onToggle={() =>
                  setExpandedProvider(current => (current === provider.id ? null : provider.id))
                }
              />
            ))}
          </div>
        </SettingsCard>

        <SettingsCard
          title={t('ai.settings.dataTitle')}
          description={t('ai.settings.dataDescription')}
          searchQuery={searchQuery}
        >
          <div className="mb-3 rounded-md border border-border/60 bg-muted/30 p-3 text-xs text-muted-foreground">
            {t(isLocal ? 'ai.settings.localDataNotice' : 'ai.settings.cloudDataNotice', {
              provider: activeProvider?.label,
            })}
          </div>
          <div className="divide-y rounded-lg border px-3">
            <div className="flex items-center justify-between gap-4 py-3">
              <div>
                <p className="text-sm font-medium">{t('ai.settings.sampleRows')}</p>
                <p className="text-xs text-muted-foreground">
                  {t('ai.settings.sampleRowsDescription')}
                </p>
              </div>
              <Switch checked={includeSampleRows} onCheckedChange={setIncludeSampleRows} />
            </div>
            <div className="flex items-center justify-between gap-4 py-3">
              <div>
                <p className="text-sm font-medium">{t('ai.settings.sensitiveData')}</p>
                <p className="text-xs text-muted-foreground">
                  {t('ai.settings.sensitiveDataDescription')}
                </p>
              </div>
              <Switch checked={allowSensitiveData} onCheckedChange={setAllowSensitiveData} />
            </div>
          </div>
        </SettingsCard>
      </div>
    </LicenseGate>
  );
}
