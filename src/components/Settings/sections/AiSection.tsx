// SPDX-License-Identifier: BUSL-1.1

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Eye, EyeOff, Trash2, Check, AlertCircle, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { SettingsCard } from '../SettingsCard';
import { LicenseGate } from '@/components/License/LicenseGate';
import { AiProviderSelector } from '@/components/AI/AiProviderSelector';
import { useAiPreferences } from '@/providers/AiPreferencesProvider';
import { aiSaveApiKey, aiDeleteApiKey, AI_PROVIDERS, type AiProvider } from '@/lib/ai';

interface AiSectionProps {
  searchQuery?: string;
}

function ProviderCard({
  provider,
  hasKey,
  onSave,
  onDelete,
}: {
  provider: (typeof AI_PROVIDERS)[number];
  hasKey: boolean;
  onSave: (provider: AiProvider, key: string) => Promise<void>;
  onDelete: (provider: AiProvider) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [key, setKey] = useState('');
  const [showKey, setShowKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [saved, setSaved] = useState(false);

  const isLocal = !provider.requiresKey;

  const handleSave = async () => {
    if (!key.trim()) return;
    setSaving(true);
    try {
      await onSave(provider.id, key.trim());
      setKey('');
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    setDeleting(true);
    try {
      await onDelete(provider.id);
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="rounded-lg border border-border p-4 space-y-3">
      <div className="flex items-center justify-between">
        <div>
          <h4 className="text-sm font-medium">{provider.label}</h4>
          <p className="text-xs text-muted-foreground">
            {t('ai.settings.defaultModel')}: {provider.defaultModel}
          </p>
        </div>
        {isLocal ? (
          <span className="text-xs px-2 py-0.5 rounded-full bg-muted text-muted-foreground">
            {t('ai.settings.local')}
          </span>
        ) : hasKey ? (
          <span className="flex items-center gap-1 text-xs px-2 py-0.5 rounded-full bg-green-500/10 text-green-600">
            <Check size={10} />
            {t('ai.settings.configured')}
          </span>
        ) : (
          <span className="flex items-center gap-1 text-xs px-2 py-0.5 rounded-full bg-warning/10 text-warning">
            <AlertCircle size={10} />
            {t('ai.settings.notConfigured')}
          </span>
        )}
      </div>

      {!isLocal && (
        <div className="space-y-2">
          <div className="flex gap-2">
            <div className="relative flex-1">
              <input
                type={showKey ? 'text' : 'password'}
                value={key}
                onChange={e => setKey(e.target.value)}
                placeholder={hasKey ? '••••••••••••' : t('ai.settings.enterKey')}
                className="w-full h-8 rounded-md border border-input bg-background px-3 pr-8 text-sm"
                onKeyDown={e => {
                  if (e.key === 'Enter') handleSave();
                }}
              />
              <button
                type="button"
                onClick={() => setShowKey(!showKey)}
                className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
              >
                {showKey ? <EyeOff size={14} /> : <Eye size={14} />}
              </button>
            </div>
            <Button size="sm" onClick={handleSave} disabled={!key.trim() || saving} className="h-8">
              {saving ? (
                <Loader2 size={14} className="animate-spin" />
              ) : saved ? (
                <Check size={14} />
              ) : (
                t('common.save')
              )}
            </Button>
          </div>

          {hasKey && (
            <Button
              variant="ghost"
              size="sm"
              onClick={handleDelete}
              disabled={deleting}
              className="h-7 text-xs text-destructive hover:text-destructive"
            >
              {deleting ? (
                <Loader2 size={12} className="animate-spin mr-1" />
              ) : (
                <Trash2 size={12} className="mr-1" />
              )}
              {t('ai.settings.deleteKey')}
            </Button>
          )}
        </div>
      )}

      {isLocal && <p className="text-xs text-muted-foreground">{t('ai.settings.ollamaHint')}</p>}
    </div>
  );
}

export function AiSection({ searchQuery }: AiSectionProps) {
  const { t } = useTranslation();
  const { preferredProvider, setPreferredProvider, providerStatuses, refreshStatuses } =
    useAiPreferences();

  const providerHasKey: Record<AiProvider, boolean> = {
    open_ai: providerStatuses.find(s => s.provider === 'open_ai')?.has_key ?? false,
    anthropic: providerStatuses.find(s => s.provider === 'anthropic')?.has_key ?? false,
    ollama: true,
  };

  const handleSave = async (provider: AiProvider, key: string) => {
    await aiSaveApiKey(provider, key);
    await refreshStatuses();
  };

  const handleDelete = async (provider: AiProvider) => {
    await aiDeleteApiKey(provider);
    await refreshStatuses();
  };

  return (
    <LicenseGate feature="ai">
      <div className="space-y-6">
        <SettingsCard
          title={t('ai.defaultProvider')}
          description={t('ai.defaultProviderDescription')}
          searchQuery={searchQuery}
        >
          <AiProviderSelector
            provider={preferredProvider}
            onProviderChange={setPreferredProvider}
            providerHasKey={providerHasKey}
          />
        </SettingsCard>

        <SettingsCard
          title={t('ai.settings.title')}
          description={t('ai.settings.description')}
          searchQuery={searchQuery}
        >
          <div className="space-y-3">
            {AI_PROVIDERS.map(provider => (
              <ProviderCard
                key={provider.id}
                provider={provider}
                hasKey={providerHasKey[provider.id]}
                onSave={handleSave}
                onDelete={handleDelete}
              />
            ))}
          </div>
        </SettingsCard>
      </div>
    </LicenseGate>
  );
}
