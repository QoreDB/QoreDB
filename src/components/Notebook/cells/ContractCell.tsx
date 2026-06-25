// SPDX-License-Identifier: BUSL-1.1

import { ChevronDown, ChevronRight, ShieldCheck } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { ContractHealthBadge, ContractResultsView } from '@/components/Contracts';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { type ContractMeta, listContracts } from '@/lib/contracts';
import type { NotebookCell } from '@/lib/notebook/notebookTypes';

interface ContractCellProps {
  cell: NotebookCell;
  onSourceChange: (source: string) => void;
}

export function ContractCell({ cell, onSourceChange }: ContractCellProps) {
  const { t } = useTranslation();
  const [contracts, setContracts] = useState<ContractMeta[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showResults, setShowResults] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    listContracts()
      .then(list => {
        if (!cancelled) setContracts(list);
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
  }, []);

  const selectedName = cell.source.trim();
  const selectedMeta = useMemo(
    () => contracts.find(c => c.name === selectedName) ?? null,
    [contracts, selectedName]
  );

  const lastRun = cell.lastResult?.contractRun ?? selectedMeta?.last_run ?? null;
  const hasRun = Boolean(cell.lastResult?.contractRun);

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <ShieldCheck size={14} className="text-muted-foreground shrink-0" />
        <Select value={selectedName} onValueChange={onSourceChange} disabled={loading}>
          <SelectTrigger className="h-8 max-w-xs text-sm">
            <SelectValue placeholder={t('contracts.list.name')} />
          </SelectTrigger>
          <SelectContent>
            {contracts.length === 0 && (
              <div className="px-2 py-1.5 text-xs text-muted-foreground">
                {loading ? t('contracts.editor.validating') : t('contracts.empty.title')}
              </div>
            )}
            {contracts.map(c => (
              <SelectItem key={c.id} value={c.name}>
                {c.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {selectedMeta && (
          <span className="text-xs text-muted-foreground">
            {t('contracts.list.rulesCount', { count: selectedMeta.rules_count })}
          </span>
        )}
        {lastRun && <ContractHealthBadge run={lastRun} />}
      </div>

      {error && (
        <div className="text-xs text-red-600 dark:text-red-400 px-2 py-1 rounded border border-red-500/30 bg-red-500/10">
          {error}
        </div>
      )}

      {hasRun && cell.lastResult?.contractRun && (
        <div className="mt-1">
          <button
            type="button"
            onClick={() => setShowResults(v => !v)}
            className="flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground transition-colors py-0.5"
          >
            {showResults ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
            <span>{showResults ? t('notebook.hideResults') : t('notebook.showResults')}</span>
          </button>
          {showResults && <ContractResultsView run={cell.lastResult.contractRun} />}
        </div>
      )}
    </div>
  );
}
