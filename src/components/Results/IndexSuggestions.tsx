// SPDX-License-Identifier: BUSL-1.1

import { ClipboardCopy, Lightbulb } from 'lucide-react';
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import type { Driver } from '@/lib/connection/drivers';
import type { PlanNode } from '@/lib/query/explainPlanParser';
import { suggestIndexesFromPlan, supportsIndexSuggestions } from '@/lib/query/indexSuggestions';
import { useLicense } from '@/providers/LicenseProvider';

interface IndexSuggestionsProps {
  root: PlanNode;
  dialect?: Driver;
}

export function IndexSuggestions({ root, dialect }: IndexSuggestionsProps) {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();

  const suggestions = useMemo(() => {
    if (!dialect || !supportsIndexSuggestions(dialect)) return [];
    return suggestIndexesFromPlan(root, dialect);
  }, [root, dialect]);

  if (!isFeatureEnabled('index_suggestions') || suggestions.length === 0) {
    return null;
  }

  const handleCopy = (sql: string) => {
    navigator.clipboard.writeText(sql);
    toast.success(t('common.copied'));
  };

  return (
    <div className="mb-2 rounded-md border border-amber-500/30 bg-amber-500/5">
      <div className="flex items-center gap-1.5 px-3 py-2 border-b border-amber-500/20 text-xs font-medium text-amber-600 dark:text-amber-400">
        <Lightbulb size={13} />
        {t('query.indexSuggestions.title')}
      </div>
      <div className="divide-y divide-border/50">
        {suggestions.map(suggestion => (
          <div key={suggestion.indexName} className="px-3 py-2 space-y-1.5">
            <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
              <span>
                {t(`query.indexSuggestions.reason.${suggestion.reason}`, {
                  table: suggestion.table,
                })}
              </span>
              {suggestion.cost !== undefined && (
                <span className="font-mono tabular-nums">
                  {t('query.indexSuggestions.cost', { value: Math.round(suggestion.cost) })}
                </span>
              )}
            </div>
            <div className="flex items-start gap-2">
              <code className="flex-1 text-xs font-mono bg-muted/50 rounded px-2 py-1.5 overflow-x-auto whitespace-pre-wrap">
                {suggestion.sql}
              </code>
              <Button
                variant="ghost"
                size="sm"
                className="h-7 gap-1.5 text-xs shrink-0"
                onClick={() => handleCopy(suggestion.sql)}
                title={t('query.indexSuggestions.copy')}
              >
                <ClipboardCopy size={12} />
              </Button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
