// SPDX-License-Identifier: BUSL-1.1

import { AlertTriangle, Sparkles } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { LicenseGate } from '@/components/License/LicenseGate';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { aiSummarizeSchema } from '@/lib/ai';
import type { Namespace } from '@/lib/tauri';
import { useAiPreferences } from '@/providers/AiPreferencesProvider';
import { AiResponseDisplay } from './AiResponseDisplay';

interface SchemaExplainDialogProps {
  sessionId: string | null;
  namespace?: Namespace | null;
  /** Explain a single table; omit to summarize the whole namespace. */
  table?: string;
  onClose: () => void;
}

export function SchemaExplainDialog({
  sessionId,
  namespace,
  table,
  onClose,
}: SchemaExplainDialogProps) {
  const { t } = useTranslation();
  const { getConfig, isReady } = useAiPreferences();
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!sessionId || !isReady) return;
    let cancelled = false;

    setLoading(true);
    setError(null);
    aiSummarizeSchema(sessionId, getConfig(), namespace ?? undefined, table)
      .then(response => {
        if (!cancelled) setContent(response.content);
      })
      .catch(err => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [sessionId, isReady, namespace, table, getConfig]);

  return (
    <Dialog open onOpenChange={value => !value && onClose()}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles size={16} className="text-accent" />
            {table
              ? t('ai.explain.tableTitle', { table })
              : t('ai.explain.schemaTitle', {
                  namespace: namespace?.schema ?? namespace?.database ?? '',
                })}
          </DialogTitle>
        </DialogHeader>

        <LicenseGate feature="ai">
          <div className="max-h-[60vh] overflow-y-auto">
            {isReady ? (
              <AiResponseDisplay
                response={content}
                loading={loading}
                generatedQuery={null}
                safetyAnalysis={null}
                error={error}
              />
            ) : (
              <p className="flex items-center gap-2 text-sm text-warning">
                <AlertTriangle size={14} />
                {t('ai.inlineEdit.notConfigured')}
              </p>
            )}
          </div>
        </LicenseGate>
      </DialogContent>
    </Dialog>
  );
}
