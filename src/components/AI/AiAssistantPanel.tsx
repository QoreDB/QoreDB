// SPDX-License-Identifier: BUSL-1.1

import { Settings, Sparkles, X } from 'lucide-react';
import { useCallback, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { LicenseGate } from '@/components/License/LicenseGate';
import { Button } from '@/components/ui/button';
import { Tooltip } from '@/components/ui/tooltip';
import { useAiAssistant } from '@/hooks/useAiAssistant';
import type { Namespace } from '@/lib/tauri';
import { useAiPreferences } from '@/providers/AiPreferencesProvider';
import { AiPromptInput } from './AiPromptInput';
import { AiResponseDisplay } from './AiResponseDisplay';

interface AiAssistantPanelProps {
  sessionId: string | null;
  namespace?: Namespace | null;
  connectionId?: string;
  onInsertQuery?: (query: string) => void;
  onClose: () => void;
  onOpenSettings?: () => void;
  pendingFix?: { query: string; error: string } | null;
  onPendingFixConsumed?: () => void;
  tableContext?: string;
}

export function AiAssistantPanel({
  sessionId,
  namespace,
  connectionId,
  onInsertQuery,
  onClose,
  onOpenSettings,
  pendingFix,
  onPendingFixConsumed,
  tableContext,
}: AiAssistantPanelProps) {
  const { t } = useTranslation();
  const { getConfig, isReady } = useAiPreferences();

  const assistant = useAiAssistant({
    sessionId,
    namespace,
    connectionId,
  });

  // Auto-trigger fix_error when pendingFix arrives
  const lastFixRef = useRef<string | null>(null);
  useEffect(() => {
    if (!pendingFix || !isReady || !sessionId) return;
    const fixKey = `${pendingFix.query}::${pendingFix.error}`;
    if (lastFixRef.current === fixKey) return;
    lastFixRef.current = fixKey;
    assistant.fixError('Fix this query error', getConfig(), pendingFix.query, pendingFix.error);
    onPendingFixConsumed?.();
  }, [pendingFix, isReady, sessionId, assistant, getConfig, onPendingFixConsumed]);

  const handleSubmit = useCallback(
    (prompt: string) => {
      assistant.generateQuery(prompt, getConfig());
    },
    [assistant, getConfig]
  );

  const placeholder = tableContext
    ? t('ai.generateForTableHint', { table: tableContext })
    : undefined;

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
          {/* No key warning */}
          {!isReady && (
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
            disabled={!sessionId || !isReady}
            placeholder={placeholder}
          />
        </div>
      </div>
    </LicenseGate>
  );
}
