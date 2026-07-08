// SPDX-License-Identifier: Apache-2.0

import { AlertCircle, Check, Loader2, RefreshCw } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import {
  DEFAULT_SEMANTIC_MODEL,
  type SemanticStatus,
  semanticReindex,
  semanticSetConfig,
  semanticStatus,
} from '@/lib/semantic';
import { openExternal } from '@/lib/transport';
import { useSessionContext } from '@/providers/SessionProvider';
import { SettingsCard } from '../SettingsCard';

export function SemanticSearchCard({ searchQuery }: { searchQuery?: string }) {
  const { t } = useTranslation();
  const { sessionId } = useSessionContext();
  const [status, setStatus] = useState<SemanticStatus | null>(null);
  const [model, setModel] = useState(DEFAULT_SEMANTIC_MODEL);
  const modelDirty = useRef(false);
  const [reindexing, setReindexing] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);

  const refreshStatus = useCallback(async () => {
    try {
      const s = await semanticStatus(sessionId ?? undefined);
      setStatus(s);
      if (!modelDirty.current) setModel(s.model);
    } catch {
      setStatus(null);
    }
  }, [sessionId]);

  useEffect(() => {
    refreshStatus();
  }, [refreshStatus]);

  const applyConfig = async (enabled: boolean, nextModel: string) => {
    const s = await semanticSetConfig({
      enabled,
      base_url: null,
      model: nextModel.trim() || DEFAULT_SEMANTIC_MODEL,
    });
    setStatus(prev => ({ ...s, index: prev?.index }));
    return s;
  };

  const handleToggle = async (enabled: boolean) => {
    await applyConfig(enabled, model);
    if (enabled && sessionId) {
      void handleReindex();
    }
  };

  const handleModelCommit = async () => {
    if (!modelDirty.current || !status) return;
    await applyConfig(status.enabled, model);
    modelDirty.current = false;
    setFeedback(t('semantic.settings.modelChangedReindex'));
  };

  const handleReindex = async () => {
    if (!sessionId || reindexing) return;
    setReindexing(true);
    setFeedback(null);
    try {
      const summary = await semanticReindex(sessionId);
      setFeedback(t('semantic.settings.reindexDone', { total: summary.total }));
    } catch (err) {
      setFeedback(`${t('semantic.settings.reindexFailed')}: ${err}`);
    } finally {
      setReindexing(false);
      void refreshStatus();
    }
  };

  const enabled = status?.enabled ?? false;

  return (
    <SettingsCard
      title={t('semantic.settings.title')}
      description={t('semantic.settings.description')}
      searchQuery={searchQuery}
    >
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <span className="text-sm">{t('semantic.settings.enable')}</span>
          <Switch checked={enabled} onCheckedChange={handleToggle} />
        </div>

        {status && (
          <div className="flex items-center gap-2 text-xs">
            {status.ollama_running ? (
              <span className="flex items-center gap-1 px-2 py-0.5 rounded-full bg-green-500/10 text-green-600">
                <Check size={10} />
                {t('semantic.settings.ollamaDetected')}
              </span>
            ) : (
              <span className="flex items-center gap-1 px-2 py-0.5 rounded-full bg-warning/10 text-warning">
                <AlertCircle size={10} />
                {t('semantic.settings.ollamaNotDetected')}
              </span>
            )}
            {status.ollama_running && !status.model_available && (
              <span className="flex items-center gap-1 px-2 py-0.5 rounded-full bg-warning/10 text-warning">
                <AlertCircle size={10} />
                {t('semantic.states.model_missing')}
              </span>
            )}
          </div>
        )}

        {status && !status.ollama_running && (
          <p className="text-xs text-muted-foreground">
            {t('semantic.settings.installHint')}{' '}
            <button
              type="button"
              className="underline hover:text-foreground"
              onClick={() => openExternal('https://ollama.com/download')}
            >
              ollama.com
            </button>
          </p>
        )}
        {status?.ollama_running && !status.model_available && (
          <p className="text-xs text-muted-foreground font-mono">
            {t('semantic.settings.pullHint', { model: model.trim() || DEFAULT_SEMANTIC_MODEL })}
          </p>
        )}

        {enabled && (
          <>
            <div className="flex items-center gap-2">
              <span className="text-sm shrink-0">{t('semantic.settings.model')}</span>
              <Input
                value={model}
                onChange={e => {
                  setModel(e.target.value);
                  modelDirty.current = true;
                }}
                onBlur={handleModelCommit}
                onKeyDown={e => {
                  if (e.key === 'Enter') handleModelCommit();
                }}
                placeholder={DEFAULT_SEMANTIC_MODEL}
                className="h-8 flex-1"
              />
            </div>

            <div className="flex items-center justify-between gap-2">
              <span className="text-xs text-muted-foreground">
                {status?.index
                  ? t('semantic.settings.objects', { total: status.index.objects })
                  : null}
              </span>
              <Button
                size="sm"
                variant="outline"
                className="h-8"
                onClick={handleReindex}
                disabled={!sessionId || reindexing || !status?.ollama_running}
              >
                {reindexing ? (
                  <Loader2 size={14} className="animate-spin mr-1" />
                ) : (
                  <RefreshCw size={14} className="mr-1" />
                )}
                {t('semantic.settings.reindex')}
              </Button>
            </div>
          </>
        )}

        {feedback && <p className="text-xs text-muted-foreground">{feedback}</p>}
      </div>
    </SettingsCard>
  );
}
