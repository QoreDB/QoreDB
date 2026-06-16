// SPDX-License-Identifier: BUSL-1.1

import { Loader2, Sparkles, X } from 'lucide-react';
import { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { aiGenerateFilters } from '@/lib/ai';
import type { ColumnFilter, FilterOperator, Namespace } from '@/lib/tauri';
import { useAiPreferences } from '@/providers/AiPreferencesProvider';

interface NaturalLanguageFilterBarProps {
  sessionId: string;
  tableName: string;
  namespace?: Namespace;
  onApply: (filters: ColumnFilter[]) => void;
}

const OPERATOR_SYMBOL: Record<FilterOperator, string> = {
  eq: '=',
  neq: '≠',
  gt: '>',
  gte: '≥',
  lt: '<',
  lte: '≤',
  like: '≈',
  is_null: 'IS NULL',
  is_not_null: 'IS NOT NULL',
  regex: '~',
  text: '⌕',
};

function formatFilter(f: ColumnFilter): string {
  if (f.operator === 'is_null' || f.operator === 'is_not_null') {
    return `${f.column} ${OPERATOR_SYMBOL[f.operator]}`;
  }
  const value = typeof f.value === 'string' ? f.value : JSON.stringify(f.value);
  return `${f.column} ${OPERATOR_SYMBOL[f.operator]} ${value}`;
}

export function NaturalLanguageFilterBar({
  sessionId,
  tableName,
  namespace,
  onApply,
}: NaturalLanguageFilterBarProps) {
  const { t } = useTranslation();
  const { getConfig } = useAiPreferences();
  const [prompt, setPrompt] = useState('');
  const [loading, setLoading] = useState(false);
  const [filters, setFilters] = useState<ColumnFilter[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleGenerate = useCallback(async () => {
    const trimmed = prompt.trim();
    if (!trimmed || loading) return;
    setLoading(true);
    setError(null);
    setFilters(null);
    try {
      const result = await aiGenerateFilters(sessionId, tableName, trimmed, getConfig(), namespace);
      setFilters(result);
    } catch (err) {
      setError(typeof err === 'string' ? err : err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [prompt, loading, sessionId, tableName, getConfig, namespace]);

  const handleApply = useCallback(() => {
    if (filters && filters.length > 0) {
      onApply(filters);
      setFilters(null);
    }
  }, [filters, onApply]);

  return (
    <div className="flex flex-col gap-1.5 px-1">
      <div className="flex items-center gap-2">
        <Sparkles size={14} className="shrink-0 text-accent" />
        <Input
          value={prompt}
          onChange={e => setPrompt(e.target.value)}
          onKeyDown={e => {
            if (e.key === 'Enter') {
              e.preventDefault();
              handleGenerate();
            }
          }}
          placeholder={t('grid.nlFilter.placeholder')}
          className="h-8"
        />
        <Button
          variant="outline"
          size="sm"
          className="h-8 shrink-0"
          onClick={handleGenerate}
          disabled={!prompt.trim() || loading}
        >
          {loading ? <Loader2 size={14} className="animate-spin" /> : t('grid.nlFilter.generate')}
        </Button>
      </div>

      {error && <div className="text-xs text-destructive px-6">{error}</div>}

      {filters && (
        <div className="flex flex-wrap items-center gap-2 px-6">
          {filters.length === 0 ? (
            <span className="text-xs text-muted-foreground">{t('grid.nlFilter.empty')}</span>
          ) : (
            <>
              {filters.map(f => (
                <span
                  key={`${f.column}-${f.operator}`}
                  className="rounded-full border border-border bg-muted px-2 py-0.5 font-mono text-[11px] text-muted-foreground"
                >
                  {formatFilter(f)}
                </span>
              ))}
              <Button size="sm" className="h-6 text-xs" onClick={handleApply}>
                {t('grid.nlFilter.apply')}
              </Button>
            </>
          )}
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6"
            onClick={() => setFilters(null)}
            title={t('common.cancel')}
          >
            <X size={12} />
          </Button>
        </div>
      )}
    </div>
  );
}
