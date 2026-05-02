// SPDX-License-Identifier: Apache-2.0

import { Check, Eye, EyeOff, Link2, Loader2, Trash2 } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  type ShareBodyMode,
  type ShareHttpMethod,
  type ShareProviderSettings,
  type ShareProviderStatus,
  shareDeleteProviderToken,
  shareGetProviderStatus,
  shareSaveProviderToken,
} from '@/lib/share/share';
import {
  DEFAULT_SHARE_PROVIDER_SETTINGS,
  getShareProviderSettings,
  setShareProviderSettings,
} from '@/lib/share/shareSettings';
import { SettingsCard } from './SettingsCard';

interface ShareProviderCardProps {
  searchQuery?: string;
}

function isModified(settings: ShareProviderSettings): boolean {
  return (
    settings.enabled !== DEFAULT_SHARE_PROVIDER_SETTINGS.enabled ||
    settings.provider_name !== DEFAULT_SHARE_PROVIDER_SETTINGS.provider_name ||
    settings.upload_url !== DEFAULT_SHARE_PROVIDER_SETTINGS.upload_url ||
    settings.method !== DEFAULT_SHARE_PROVIDER_SETTINGS.method ||
    settings.body_mode !== DEFAULT_SHARE_PROVIDER_SETTINGS.body_mode ||
    settings.file_field_name !== DEFAULT_SHARE_PROVIDER_SETTINGS.file_field_name ||
    settings.response_url_path !== DEFAULT_SHARE_PROVIDER_SETTINGS.response_url_path
  );
}

export function ShareProviderCard({ searchQuery }: ShareProviderCardProps) {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<ShareProviderSettings>(() => getShareProviderSettings());
  const [status, setStatus] = useState<ShareProviderStatus>({ has_token: false });
  const [token, setToken] = useState('');
  const [showToken, setShowToken] = useState(false);
  const [savingToken, setSavingToken] = useState(false);
  const [deletingToken, setDeletingToken] = useState(false);
  const [savedToken, setSavedToken] = useState(false);

  async function refreshStatus() {
    try {
      const nextStatus = await shareGetProviderStatus();
      setStatus(nextStatus);
    } catch {
      setStatus({ has_token: false });
    }
  }

  useEffect(() => {
    refreshStatus();
  }, []);

  const providerConfigured = useMemo(
    () => settings.enabled && settings.upload_url.trim().length > 0,
    [settings.enabled, settings.upload_url]
  );

  function updateSettings(patch: Partial<ShareProviderSettings>) {
    const next = { ...settings, ...patch };
    setSettings(next);
    setShareProviderSettings(next);
  }

  async function handleSaveToken() {
    if (!token.trim()) return;

    setSavingToken(true);
    try {
      await shareSaveProviderToken(token.trim());
      setToken('');
      setSavedToken(true);
      await refreshStatus();
      setTimeout(() => setSavedToken(false), 2000);
    } catch (error) {
      toast.error(t('share.settings.tokenSaveError'), {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setSavingToken(false);
    }
  }

  async function handleDeleteToken() {
    setDeletingToken(true);
    try {
      await shareDeleteProviderToken();
      await refreshStatus();
    } catch (error) {
      toast.error(t('share.settings.tokenDeleteError'), {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setDeletingToken(false);
    }
  }

  return (
    <SettingsCard
      id="share-provider"
      title={t('share.settings.title')}
      description={t('share.settings.description')}
      isModified={isModified(settings)}
      searchQuery={searchQuery}
    >
      <div className="space-y-4">
        <label className="flex items-start gap-2.5 text-sm cursor-pointer">
          <Checkbox
            checked={settings.enabled}
            onCheckedChange={checked => updateSettings({ enabled: Boolean(checked) })}
            className="mt-0.5"
          />
          <span>
            <span className="font-medium text-foreground">{t('share.settings.enabled')}</span>
            <span className="block text-xs text-muted-foreground mt-0.5">
              {t('share.settings.enabledDescription')}
            </span>
          </span>
        </label>

        <div className="grid gap-4 md:grid-cols-2">
          <div className="space-y-2">
            <Label htmlFor="share-provider-name">{t('share.settings.providerName')}</Label>
            <Input
              id="share-provider-name"
              value={settings.provider_name}
              onChange={event => updateSettings({ provider_name: event.target.value })}
              placeholder={t('share.settings.providerNamePlaceholder')}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="share-provider-url">{t('share.settings.uploadUrl')}</Label>
            <Input
              id="share-provider-url"
              value={settings.upload_url}
              onChange={event => updateSettings({ upload_url: event.target.value })}
              placeholder={t('share.settings.uploadUrlPlaceholder')}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="share-provider-method">{t('share.settings.method')}</Label>
            <Select
              value={settings.method}
              onValueChange={value => updateSettings({ method: value as ShareHttpMethod })}
            >
              <SelectTrigger id="share-provider-method">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="post">POST</SelectItem>
                <SelectItem value="put">PUT</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <Label htmlFor="share-provider-mode">{t('share.settings.bodyMode')}</Label>
            <Select
              value={settings.body_mode}
              onValueChange={value => updateSettings({ body_mode: value as ShareBodyMode })}
            >
              <SelectTrigger id="share-provider-mode">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="multipart">{t('share.settings.bodyModeMultipart')}</SelectItem>
                <SelectItem value="binary">{t('share.settings.bodyModeBinary')}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {settings.body_mode === 'multipart' && (
            <div className="space-y-2">
              <Label htmlFor="share-provider-field">{t('share.settings.fileField')}</Label>
              <Input
                id="share-provider-field"
                value={settings.file_field_name}
                onChange={event => updateSettings({ file_field_name: event.target.value })}
                placeholder={t('share.settings.fileFieldPlaceholder')}
              />
            </div>
          )}

          <div className="space-y-2">
            <Label htmlFor="share-provider-path">{t('share.settings.responsePath')}</Label>
            <Input
              id="share-provider-path"
              value={settings.response_url_path}
              onChange={event => updateSettings({ response_url_path: event.target.value })}
              placeholder={t('share.settings.responsePathPlaceholder')}
            />
          </div>
        </div>

        <div className="rounded-md border border-border bg-muted/20 p-3 space-y-3">
          <div className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <Link2 size={15} className="text-accent" />
              <div>
                <p className="text-sm font-medium text-foreground">
                  {t('share.settings.tokenTitle')}
                </p>
                <p className="text-xs text-muted-foreground">
                  {status.has_token
                    ? t('share.settings.tokenConfigured')
                    : t('share.settings.tokenOptional')}
                </p>
              </div>
            </div>
            {providerConfigured && (
              <span className="text-xs px-2 py-0.5 rounded-full bg-emerald-500/10 text-emerald-600">
                {t('share.settings.ready')}
              </span>
            )}
          </div>

          <div className="flex gap-2">
            <div className="relative flex-1">
              <Input
                type={showToken ? 'text' : 'password'}
                value={token}
                onChange={event => setToken(event.target.value)}
                placeholder={
                  status.has_token ? '••••••••••••' : t('share.settings.tokenPlaceholder')
                }
                className="pr-8"
                onKeyDown={event => {
                  if (event.key === 'Enter') {
                    void handleSaveToken();
                  }
                }}
              />
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="absolute right-1 top-1/2 h-6 w-6 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                onClick={() => setShowToken(value => !value)}
              >
                {showToken ? <EyeOff size={14} /> : <Eye size={14} />}
              </Button>
            </div>

            <Button
              type="button"
              size="sm"
              className="shrink-0"
              onClick={() => void handleSaveToken()}
              disabled={!token.trim() || savingToken}
            >
              {savingToken ? (
                <Loader2 size={14} className="animate-spin" />
              ) : savedToken ? (
                <Check size={14} />
              ) : (
                t('common.save')
              )}
            </Button>

            {status.has_token && (
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="shrink-0"
                onClick={() => void handleDeleteToken()}
                disabled={deletingToken}
              >
                {deletingToken ? (
                  <Loader2 size={14} className="animate-spin" />
                ) : (
                  <Trash2 size={14} />
                )}
              </Button>
            )}
          </div>

          <p className="text-xs text-muted-foreground">{t('share.settings.responseHint')}</p>
        </div>
      </div>
    </SettingsCard>
  );
}
