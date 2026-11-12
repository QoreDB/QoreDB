// SPDX-License-Identifier: BUSL-1.1

import { FileText, Sparkles } from 'lucide-react';
import { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AiResponseDisplay } from '@/components/AI/AiResponseDisplay';
import { Button } from '@/components/ui/button';
import { aiExplainResult } from '@/lib/ai';
import type { CellResult } from '@/lib/notebook/notebookTypes';
import type { Namespace } from '@/lib/tauri';
import { useAiPreferences } from '@/providers/AiPreferencesProvider';
import { useLicense } from '@/providers/LicenseProvider';

interface CellResultSummaryProps {
  result: CellResult;
  query: string;
  sessionId?: string | null;
  namespace?: Namespace | null;
  onInsertMarkdownBelow?: (markdown: string) => void;
}

export function CellResultSummary({
  result,
  query,
  sessionId,
  namespace,
  onInsertMarkdownBelow,
}: CellResultSummaryProps) {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();
  const { getConfig, isReady } = useAiPreferences();

  const [summary, setSummary] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const canSummarize =
    isFeatureEnabled('ai') &&
    isReady &&
    Boolean(sessionId) &&
    result.type === 'table' &&
    Boolean(result.columns?.length);

  const handleSummarize = useCallback(async () => {
    if (!sessionId || !result.columns || loading) return;
    setLoading(true);
    setError(null);
    try {
      const cols = result.columns.map(c => c.name).join(', ');
      const rowCount = result.totalRows ?? result.rows?.length ?? 0;
      const resultSummary = `${rowCount} rows, ${result.columns.length} columns (${cols})`;
      const response = await aiExplainResult(
        sessionId,
        query,
        resultSummary,
        getConfig(),
        namespace ?? undefined
      );
      setSummary(response.content);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [sessionId, result, query, getConfig, namespace, loading]);

  if (!canSummarize) return null;

  return (
    <div className="mt-2">
      {!summary && !loading && (
        <Button
          variant="ghost"
          size="sm"
          onClick={handleSummarize}
          className="h-7 gap-1.5 text-xs text-muted-foreground hover:text-foreground"
        >
          <Sparkles size={12} />
          {t('ai.summarizeResults')}
        </Button>
      )}
      {(loading || summary || error) && (
        <div className="space-y-1.5">
          <AiResponseDisplay
            response={summary ?? ''}
            loading={loading}
            generatedQuery={null}
            safetyAnalysis={null}
            error={error}
          />
          {summary && onInsertMarkdownBelow && (
            <Button
              variant="outline"
              size="sm"
              onClick={() => onInsertMarkdownBelow(summary)}
              className="h-6 gap-1.5 text-[11px]"
            >
              <FileText size={11} />
              {t('notebook.insertSummaryMarkdown')}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
