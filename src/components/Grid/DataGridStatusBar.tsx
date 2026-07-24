// SPDX-License-Identifier: Apache-2.0

import { Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';

interface DataGridStatusBarProps {
  loadedRows: number;
  totalRows: number;
  totalRowsExact: boolean;
  isFetchingMore: boolean;
  isCountingTotal: boolean;
  isComplete: boolean;
  onCalculateExactTotal?: () => void;
}

export function DataGridStatusBar({
  loadedRows,
  totalRows,
  totalRowsExact,
  isFetchingMore,
  isCountingTotal,
  isComplete,
  onCalculateExactTotal,
}: DataGridStatusBarProps) {
  const { t } = useTranslation();
  const percentage =
    totalRowsExact && totalRows > 0 ? Math.min(100, Math.round((loadedRows / totalRows) * 100)) : 0;

  return (
    <div
      aria-live="polite"
      className="flex items-center justify-between px-2 py-1 border-t border-border bg-muted/20"
    >
      <div className="flex items-center gap-3 text-xs text-muted-foreground">
        <span>
          {isComplete
            ? t('grid.infiniteScroll.allLoaded', { total: loadedRows.toLocaleString() })
            : totalRowsExact
              ? t('grid.infiniteScroll.loaded', {
                  loaded: loadedRows.toLocaleString(),
                  total: totalRows.toLocaleString(),
                })
              : t('grid.infiniteScroll.loadedUnknown', {
                  loaded: loadedRows.toLocaleString(),
                })}
        </span>
        {!isComplete && totalRowsExact && totalRows > 0 && (
          <div className="w-24 h-1.5 bg-muted rounded-full overflow-hidden">
            <div
              className="h-full bg-(--q-accent) rounded-full transition-all duration-300"
              style={{ width: `${percentage}%` }}
            />
          </div>
        )}
      </div>
      <div className="flex items-center gap-3 text-xs text-muted-foreground">
        {!isComplete && !totalRowsExact && onCalculateExactTotal && (
          <button
            type="button"
            disabled={isCountingTotal}
            onClick={onCalculateExactTotal}
            className="inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded px-1.5 py-0.5 hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-60"
          >
            {isCountingTotal && <Loader2 size={12} className="animate-spin" />}
            {isCountingTotal
              ? t('grid.infiniteScroll.countingTotal')
              : t('grid.infiniteScroll.calculateTotal')}
          </button>
        )}
        {isFetchingMore && (
          <span className="flex items-center gap-1.5">
            <Loader2 size={12} className="animate-spin" />
            {t('grid.infiniteScroll.loading')}
          </span>
        )}
      </div>
    </div>
  );
}
