// SPDX-License-Identifier: Apache-2.0

import { AlertTriangle, SlidersHorizontal } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import type { CostReason, SearchMode } from '@/lib/query/indexCost';
import type { TableColumn } from '@/lib/tauri';
import { cn } from '@/lib/utils';

export interface SearchScopeState {
  /** Every column of the table, so the scope can be widened again. */
  columns: TableColumn[];
  selected: string[];
  mode: SearchMode;
  /** Null when the search can reach an index. */
  cost: CostReason;
  onSelectedChange: (columns: string[]) => void;
  onModeChange: (mode: SearchMode) => void;
}

const MODES: SearchMode[] = ['contains', 'starts_with'];

export function SearchScopeControl({ scope }: { scope: SearchScopeState }) {
  const { t } = useTranslation();
  const { columns, selected, mode, cost } = scope;

  if (columns.length === 0) return null;

  const selectedSet = new Set(selected);

  function toggle(name: string) {
    const next = selectedSet.has(name) ? selected.filter(col => col !== name) : [...selected, name];
    // An empty scope would mean "search nothing"; keep the last column instead.
    if (next.length > 0) scope.onSelectedChange(next);
  }

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          title={t('grid.searchScope.title')}
          className={cn('h-7 w-7 shrink-0', cost && 'text-warning')}
        >
          <SlidersHorizontal size={14} />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-72 p-0">
        <div className="border-b border-border px-3 py-2">
          <p className="text-xs font-medium">{t('grid.searchScope.title')}</p>
          <p className="mt-0.5 text-[11px] text-muted-foreground">
            {t('grid.searchScope.description')}
          </p>
        </div>

        <div className="flex gap-1 border-b border-border px-3 py-2">
          {MODES.map(value => (
            <Button
              key={value}
              variant="ghost"
              size="sm"
              onClick={() => scope.onModeChange(value)}
              className={cn('h-6 flex-1 text-[11px]', mode === value && 'bg-accent/20 text-accent')}
            >
              {t(`grid.searchScope.mode.${value}`)}
            </Button>
          ))}
        </div>

        {cost && (
          <p className="flex items-start gap-1.5 border-b border-border bg-warning/10 px-3 py-2 text-[11px] text-muted-foreground">
            <AlertTriangle size={12} className="mt-0.5 shrink-0 text-warning" />
            {t(`grid.searchScope.cost.${cost}`)}
          </p>
        )}

        <div className="max-h-60 overflow-auto py-1">
          {columns.map(column => (
            <label
              key={column.name}
              htmlFor={`search-scope-${column.name}`}
              className="flex cursor-pointer items-center gap-2 px-3 py-1 text-xs hover:bg-muted/40"
            >
              <Checkbox
                id={`search-scope-${column.name}`}
                checked={selectedSet.has(column.name)}
                onCheckedChange={() => toggle(column.name)}
              />
              <span className="truncate">{column.name}</span>
              <span className="ml-auto shrink-0 text-[10px] text-muted-foreground">
                {column.data_type}
              </span>
            </label>
          ))}
        </div>
      </PopoverContent>
    </Popover>
  );
}
