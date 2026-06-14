// SPDX-License-Identifier: BUSL-1.1

import { AlertTriangle, ArrowDownToLine, ClipboardCopy, Loader2, ShieldAlert } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import type { SafetyInfo } from '@/lib/ai';

interface AiResponseDisplayProps {
  response: string;
  loading: boolean;
  generatedQuery: string | null;
  safetyAnalysis: SafetyInfo | null;
  error: string | null;
  onInsertQuery?: (query: string) => void;
}

export function AiResponseDisplay({
  response,
  loading,
  generatedQuery,
  safetyAnalysis,
  error,
  onInsertQuery,
}: AiResponseDisplayProps) {
  const { t } = useTranslation();

  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    toast.success(t('ai.copyQuery'));
  };

  if (error) {
    return (
      <div className="flex items-start gap-2 p-3 rounded-md bg-destructive/10 text-destructive text-sm">
        <AlertTriangle size={16} className="mt-0.5 shrink-0" />
        <span>{error}</span>
      </div>
    );
  }

  if (!response && !loading) {
    return null;
  }

  return (
    <div className="flex flex-col gap-2">
      {/* Response text */}
      <div className="text-sm leading-relaxed">
        <div className="prose prose-sm dark:prose-invert max-w-none [&_pre]:whitespace-pre-wrap">
          <Markdown remarkPlugins={[remarkGfm]}>{response}</Markdown>
        </div>
        {loading && (
          <Loader2 size={14} className="inline-block ml-1 animate-spin text-muted-foreground" />
        )}
      </div>

      {/* Generated query block */}
      {generatedQuery && (
        <div className="rounded-md border border-border bg-muted/30 overflow-hidden">
          {/* Safety badges */}
          {safetyAnalysis && (safetyAnalysis.is_mutation || safetyAnalysis.is_dangerous) && (
            <div className="flex items-center gap-2 px-3 py-1.5 border-b border-border bg-muted/50">
              {safetyAnalysis.is_dangerous && (
                <span className="flex items-center gap-1 text-xs font-medium text-destructive">
                  <ShieldAlert size={12} />
                  {t('ai.dangerousWarning')}
                </span>
              )}
              {safetyAnalysis.is_mutation && !safetyAnalysis.is_dangerous && (
                <span className="flex items-center gap-1 text-xs font-medium text-warning">
                  <AlertTriangle size={12} />
                  {t('ai.safetyWarning')}
                </span>
              )}
            </div>
          )}

          {/* Query code */}
          <pre className="p-3 text-sm font-mono overflow-x-auto whitespace-pre-wrap">
            {generatedQuery}
          </pre>

          {/* Action buttons */}
          <div className="flex items-center gap-2 px-3 py-2 border-t border-border">
            {onInsertQuery && (
              <Button
                size="sm"
                variant="default"
                className="h-7 gap-1.5 text-xs"
                onClick={() => onInsertQuery(generatedQuery)}
              >
                <ArrowDownToLine size={12} />
                {t('ai.insertQuery')}
              </Button>
            )}
            <Button
              size="sm"
              variant="ghost"
              className="h-7 gap-1.5 text-xs"
              onClick={() => handleCopy(generatedQuery)}
            >
              <ClipboardCopy size={12} />
              {t('ai.copyQuery')}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
