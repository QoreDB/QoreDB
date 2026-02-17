// SPDX-License-Identifier: BUSL-1.1

import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Sparkles, X, Settings } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Tooltip } from '@/components/ui/tooltip';
import { LicenseGate } from '@/components/License/LicenseGate';
import { useAiAssistant } from '@/hooks/useAiAssistant';
import { AiPromptInput } from './AiPromptInput';
import { AiResponseDisplay } from './AiResponseDisplay';
import { AiProviderSelector } from './AiProviderSelector';
import {
  aiGetProviderStatus,
  type AiConfig,
  type AiProvider,
  type AiProviderStatus,
} from '@/lib/ai';
import type { Namespace } from '@/lib/tauri';

interface AiAssistantPanelProps {
  sessionId: string | null;
  namespace?: Namespace | null;
  connectionId?: string;
  onInsertQuery?: (query: string) => void;
  onClose: () => void;
  onOpenSettings?: () => void;
}

const STORAGE_KEY = 'qoredb_ai_provider';

function loadSavedProvider(): AiProvider {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === 'open_ai' || saved === 'anthropic' || saved === 'ollama') {
      return saved;
    }
  } catch {
    // ignore
  }
  return 'open_ai';
}

export function AiAssistantPanel({
  sessionId,
  namespace,
  connectionId,
  onInsertQuery,
  onClose,
  onOpenSettings,
}: AiAssistantPanelProps) {
  const { t } = useTranslation();
  const [provider, setProvider] = useState<AiProvider>(loadSavedProvider);
  const [providerStatuses, setProviderStatuses] = useState<AiProviderStatus[]>([]);

  const assistant = useAiAssistant({
    sessionId,
    namespace,
    connectionId,
  });

  // Load provider statuses on mount
  useEffect(() => {
    aiGetProviderStatus()
      .then(setProviderStatuses)
      .catch(() => {});
  }, []);

  const providerHasKey: Record<AiProvider, boolean> = {
    open_ai: providerStatuses.find(s => s.provider === 'open_ai')?.has_key ?? false,
    anthropic: providerStatuses.find(s => s.provider === 'anthropic')?.has_key ?? false,
    ollama: true,
  };

  const handleProviderChange = useCallback((p: AiProvider) => {
    setProvider(p);
    localStorage.setItem(STORAGE_KEY, p);
  }, []);

  const handleSubmit = useCallback(
    (prompt: string) => {
      const config: AiConfig = { provider };
      assistant.generateQuery(prompt, config);
    },
    [provider, assistant]
  );

  const currentProviderReady = providerHasKey[provider];

  return (
    <LicenseGate feature="ai">
      <div className="flex flex-col h-full border-l border-border bg-background">
        {/* Header */}
        <div className="flex items-center justify-between px-3 py-2 border-b border-border bg-muted/20">
          <div className="flex items-center gap-2">
            <Sparkles size={14} className="text-accent" />
            <span className="text-xs font-medium">{t('ai.title')}</span>
          </div>
          <div className="flex items-center gap-1">
            {onOpenSettings && (
              <Tooltip content={t('ai.configureProvider')}>
                <Button variant="ghost" size="icon" className="h-6 w-6" onClick={onOpenSettings}>
                  <Settings size={12} />
                </Button>
              </Tooltip>
            )}
            <Button variant="ghost" size="icon" className="h-6 w-6" onClick={onClose}>
              <X size={12} />
            </Button>
          </div>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-auto p-3 space-y-3">
          {/* Provider selector */}
          <AiProviderSelector
            provider={provider}
            onProviderChange={handleProviderChange}
            providerHasKey={providerHasKey}
          />

          {/* No key warning */}
          {!currentProviderReady && (
            <div className="text-xs text-warning bg-warning/10 px-3 py-2 rounded-md border border-warning/20">
              {t('ai.noProvider')}{' '}
              {onOpenSettings && (
                <button className="underline cursor-pointer" onClick={onOpenSettings}>
                  {t('ai.configureProvider')}
                </button>
              )}
            </div>
          )}

          {/* No session warning */}
          {!sessionId && (
            <div className="text-xs text-muted-foreground bg-muted/50 px-3 py-2 rounded-md">
              {t('ai.noSession')}
            </div>
          )}

          {/* Response */}
          <AiResponseDisplay
            response={assistant.response}
            loading={assistant.loading}
            generatedQuery={assistant.generatedQuery}
            safetyAnalysis={assistant.safetyAnalysis}
            error={assistant.error}
            onInsertQuery={onInsertQuery}
          />
        </div>

        {/* Prompt input at bottom */}
        <div className="p-3 border-t border-border">
          <AiPromptInput
            onSubmit={handleSubmit}
            loading={assistant.loading}
            disabled={!sessionId || !currentProviderReady}
          />
        </div>
      </div>
    </LicenseGate>
  );
}
