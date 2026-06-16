// SPDX-License-Identifier: BUSL-1.1

import { Loader2, Sparkles } from 'lucide-react';
import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { AiMessageThread } from '@/components/AI/AiMessageThread';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { useAiAssistant } from '@/hooks/useAiAssistant';
import type { NotebookCell } from '@/lib/notebook/notebookTypes';
import type { Namespace } from '@/lib/tauri';
import { useAiPreferences } from '@/providers/AiPreferencesProvider';

interface AiCellProps {
  cell: NotebookCell;
  sessionId?: string | null;
  namespace?: Namespace | null;
  onSourceChange: (source: string) => void;
  onInsertSqlBelow?: (source: string) => void;
}

export function AiCell({
  cell,
  sessionId,
  namespace,
  onSourceChange,
  onInsertSqlBelow,
}: AiCellProps) {
  const { t } = useTranslation();
  const { getConfig, isReady } = useAiPreferences();
  const assistant = useAiAssistant({ sessionId: sessionId ?? null, namespace });

  const canGenerate = !!sessionId && isReady && !!cell.source.trim() && !assistant.loading;

  const handleGenerate = useCallback(() => {
    if (!canGenerate) return;
    assistant.generateQuery(cell.source.trim(), getConfig());
  }, [canGenerate, assistant, cell.source, getConfig]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        handleGenerate();
      }
    },
    [handleGenerate]
  );

  const handleInsert = useCallback(
    (query: string) => {
      onInsertSqlBelow?.(query);
      toast.success(t('notebook.aiCellInserted'));
    },
    [onInsertSqlBelow, t]
  );

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-start gap-2">
        <Textarea
          value={cell.source}
          onChange={e => onSourceChange(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={t('notebook.aiPlaceholder')}
          className="min-h-[60px] resize-y text-sm"
          rows={2}
        />
        <Button
          size="icon"
          onClick={handleGenerate}
          disabled={!canGenerate}
          title={t('notebook.aiGenerate')}
          className="h-9 w-9 shrink-0"
        >
          {assistant.loading ? (
            <Loader2 size={16} className="animate-spin" />
          ) : (
            <Sparkles size={16} />
          )}
        </Button>
      </div>

      {!sessionId && (
        <div className="text-xs text-muted-foreground bg-muted/50 px-3 py-2 rounded-md">
          {t('ai.noSession')}
        </div>
      )}
      {sessionId && !isReady && (
        <div className="text-xs text-warning bg-warning/10 px-3 py-2 rounded-md border border-warning/20">
          {t('ai.noProvider')}
        </div>
      )}

      <AiMessageThread
        items={assistant.items}
        onInsertQuery={onInsertSqlBelow ? handleInsert : undefined}
      />
    </div>
  );
}
