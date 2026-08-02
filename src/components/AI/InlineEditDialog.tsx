// SPDX-License-Identifier: BUSL-1.1

import { AlertTriangle, Check, Loader2, Sparkles, X } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { LicenseGate } from '@/components/License/LicenseGate';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Textarea } from '@/components/ui/textarea';
import { useAiAssistant } from '@/hooks/useAiAssistant';
import { countChanges, diffLines } from '@/lib/query/inlineEditDiff';
import type { Namespace } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { useAiPreferences } from '@/providers/AiPreferencesProvider';

interface InlineEditDialogProps {
  open: boolean;
  /** Text handed to the model: the selection, or the whole editor content. */
  source: string;
  /** True when `source` is a selection rather than the full query. */
  isSelection: boolean;
  sessionId: string | null;
  namespace?: Namespace | null;
  onApply: (rewritten: string) => void;
  onClose: () => void;
}

export function InlineEditDialog({
  open,
  source,
  isSelection,
  sessionId,
  namespace,
  onApply,
  onClose,
}: InlineEditDialogProps) {
  const { t } = useTranslation();
  const { getConfig, isReady, includeSampleRows } = useAiPreferences();
  const [instruction, setInstruction] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const assistant = useAiAssistant({
    sessionId,
    namespace,
    getEditorContext: () => ({ current_query: source }),
    includeSampleRows,
  });

  useEffect(() => {
    if (open) {
      setInstruction('');
      assistant.reset();
      // The dialog owns focus once mounted; the editor keeps its selection.
      requestAnimationFrame(() => textareaRef.current?.focus());
    }
  }, [open, assistant.reset]);

  const last = assistant.items[assistant.items.length - 1];
  const answer = last?.role === 'assistant' ? last : null;
  const rewritten = answer?.generatedQuery ?? null;

  const submit = useCallback(() => {
    const trimmed = instruction.trim();
    if (!trimmed || assistant.loading || !isReady) return;
    assistant.generateQuery(
      `Rewrite the query currently in the editor to satisfy this instruction. Reply with the rewritten query only.\n\nInstruction: ${trimmed}`,
      getConfig()
    );
  }, [instruction, assistant, isReady, getConfig]);

  const apply = useCallback(() => {
    if (!rewritten) return;
    onApply(rewritten);
    onClose();
  }, [rewritten, onApply, onClose]);

  const lines = rewritten ? diffLines(source, rewritten) : [];
  const changes = countChanges(lines);
  const identical = rewritten !== null && changes.added === 0 && changes.removed === 0;

  return (
    <Dialog open={open} onOpenChange={value => !value && onClose()}>
      <DialogContent className="max-w-3xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles size={16} className="text-accent" />
            {isSelection ? t('ai.inlineEdit.titleSelection') : t('ai.inlineEdit.title')}
          </DialogTitle>
        </DialogHeader>

        <LicenseGate feature="ai">
          <div className="space-y-3">
            <Textarea
              ref={textareaRef}
              value={instruction}
              onChange={e => setInstruction(e.target.value)}
              onKeyDown={e => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  submit();
                }
              }}
              placeholder={t('ai.inlineEdit.placeholder')}
              disabled={assistant.loading || !isReady}
              className="min-h-[60px] max-h-[120px] resize-none text-sm"
              rows={2}
            />

            {!isReady && (
              <p className="flex items-center gap-2 text-sm text-warning">
                <AlertTriangle size={14} />
                {t('ai.inlineEdit.notConfigured')}
              </p>
            )}

            {assistant.loading && (
              <p className="flex items-center gap-2 text-sm text-muted-foreground">
                <Loader2 size={14} className="animate-spin" />
                {t('ai.inlineEdit.working')}
              </p>
            )}

            {answer?.error && (
              <p className="flex items-center gap-2 text-sm text-destructive">
                <AlertTriangle size={14} />
                {answer.error}
              </p>
            )}

            {answer && !assistant.loading && !answer.error && !rewritten && (
              <p className="text-sm text-muted-foreground">{t('ai.inlineEdit.noQuery')}</p>
            )}

            {rewritten && !assistant.loading && (
              <>
                <div className="flex items-center gap-3 text-xs text-muted-foreground">
                  <span className="text-success">+{changes.added}</span>
                  <span className="text-destructive">-{changes.removed}</span>
                  {identical && <span>{t('ai.inlineEdit.identical')}</span>}
                  {answer?.safetyAnalysis?.is_dangerous && (
                    <span className="flex items-center gap-1 text-warning">
                      <AlertTriangle size={12} />
                      {t('ai.inlineEdit.dangerous')}
                    </span>
                  )}
                </div>

                <div className="max-h-[40vh] overflow-auto rounded-md border border-border bg-muted/30">
                  <pre className="p-2 text-xs font-mono leading-relaxed">
                    {lines.map((line, index) => (
                      <div
                        // biome-ignore lint/suspicious/noArrayIndexKey: diff lines are positional and rebuilt wholesale on each answer
                        key={index}
                        className={cn(
                          'px-2 -mx-2',
                          line.kind === 'added' && 'bg-success/10 text-success',
                          line.kind === 'removed' && 'bg-destructive/10 text-destructive'
                        )}
                      >
                        <span className="select-none opacity-60">
                          {line.kind === 'added' ? '+' : line.kind === 'removed' ? '-' : ' '}{' '}
                        </span>
                        {line.text || ' '}
                      </div>
                    ))}
                  </pre>
                </div>
              </>
            )}
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={onClose}>
              <X size={14} />
              {t('common.cancel')}
            </Button>
            {rewritten && !assistant.loading ? (
              <Button onClick={apply} disabled={identical}>
                <Check size={14} />
                {isSelection ? t('ai.inlineEdit.applySelection') : t('ai.inlineEdit.apply')}
              </Button>
            ) : (
              <Button
                onClick={submit}
                disabled={!instruction.trim() || assistant.loading || !isReady}
              >
                <Sparkles size={14} />
                {t('ai.inlineEdit.generate')}
              </Button>
            )}
          </DialogFooter>
        </LicenseGate>
      </DialogContent>
    </Dialog>
  );
}
