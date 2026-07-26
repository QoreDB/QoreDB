// SPDX-License-Identifier: Apache-2.0

import { AlertTriangle, Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { CancelSupport, OrderingGuarantee, TotalRowsSource } from '@/lib/tauri';

interface DataGridStatusBarProps {
  loadedRows: number;
  totalRows: number | null;
  totalRowsSource?: TotalRowsSource | null;
  totalRowsAsOf?: number | null;
  isFetchingMore: boolean;
  isCountingTotal: boolean;
  isComplete: boolean;
  /** Scrolling stopped at the engine's window, not at the end of the data. */
  windowExhausted?: boolean;
  /** Whether rows appear exactly once across pages. */
  orderingGuarantee?: OrderingGuarantee;
  /** What the driver can actually promise when the count is cancelled. */
  cancelSupport?: CancelSupport;
  onCalculateExactTotal?: () => void;
  onCancelExactTotal?: () => void;
}

export function DataGridStatusBar({
  loadedRows,
  totalRows,
  totalRowsSource,
  totalRowsAsOf,
  isFetchingMore,
  isCountingTotal,
  isComplete,
  windowExhausted,
  orderingGuarantee,
  cancelSupport,
  onCalculateExactTotal,
  onCancelExactTotal,
}: DataGridStatusBarProps) {
  const { t } = useTranslation();
  const isEstimate = totalRowsSource === 'estimated';
  // Progress against an approximation would read as precision it does not have.
  const showProgress = totalRows !== null && totalRows > 0 && !isEstimate;
  const percentage =
    totalRows && totalRows > 0 ? Math.min(100, Math.round((loadedRows / totalRows) * 100)) : 0;

  return (
    <div
      aria-live="polite"
      className="flex items-center justify-between px-2 py-1 border-t border-border bg-muted/20"
    >
      <div className="flex items-center gap-3 text-xs text-muted-foreground">
        <span
          title={
            isEstimate
              ? [
                  t('grid.infiniteScroll.estimateHint'),
                  totalRowsAsOf
                    ? t('grid.infiniteScroll.estimateAsOf', {
                        date: new Date(totalRowsAsOf).toLocaleString(),
                      })
                    : null,
                ]
                  .filter(Boolean)
                  .join('\n')
              : undefined
          }
        >
          {isComplete
            ? windowExhausted
              ? t('grid.infiniteScroll.windowExhausted', { loaded: loadedRows.toLocaleString() })
              : t('grid.infiniteScroll.allLoaded', { total: loadedRows.toLocaleString() })
            : totalRows === null
              ? t('grid.infiniteScroll.loadedUnknown', {
                  loaded: loadedRows.toLocaleString(),
                })
              : isEstimate
                ? t('grid.infiniteScroll.loadedEstimated', {
                    loaded: loadedRows.toLocaleString(),
                    total: totalRows.toLocaleString(),
                  })
                : t('grid.infiniteScroll.loaded', {
                    loaded: loadedRows.toLocaleString(),
                    total: totalRows.toLocaleString(),
                  })}
        </span>
        {orderingGuarantee === 'none' && loadedRows > 0 && (
          <span
            className="flex items-center gap-1 text-warning/90"
            title={t('grid.pagination.unstableOrderHint')}
          >
            <AlertTriangle size={12} />
            {t('grid.pagination.unstableOrder')}
          </span>
        )}
        {!isComplete && showProgress && (
          <div className="w-24 h-1.5 bg-muted rounded-full overflow-hidden">
            <div
              className="h-full bg-(--q-accent) rounded-full transition-all duration-300"
              style={{ width: `${percentage}%` }}
            />
          </div>
        )}
      </div>
      <div className="flex items-center gap-3 text-xs text-muted-foreground">
        {!isComplete &&
          (totalRows === null || isEstimate) &&
          onCalculateExactTotal &&
          (isCountingTotal ? (
            <span className="inline-flex shrink-0 items-center gap-2 whitespace-nowrap">
              <span className="inline-flex items-center gap-1.5 px-1.5 py-0.5">
                <Loader2 size={12} className="animate-spin" />
                {t('grid.infiniteScroll.countingTotal')}
              </span>
              {onCancelExactTotal && cancelSupport !== 'none' && (
                <button
                  type="button"
                  onClick={onCancelExactTotal}
                  title={
                    cancelSupport === 'best_effort'
                      ? t('grid.infiniteScroll.cancelCountBestEffort')
                      : undefined
                  }
                  className="rounded px-1.5 py-0.5 underline underline-offset-2 hover:bg-muted hover:text-foreground"
                >
                  {cancelSupport === 'best_effort'
                    ? t('grid.infiniteScroll.cancelCountBestEffortLabel')
                    : t('grid.infiniteScroll.cancelCount')}
                </button>
              )}
            </span>
          ) : (
            <button
              type="button"
              onClick={onCalculateExactTotal}
              className="inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded px-1.5 py-0.5 hover:bg-muted hover:text-foreground"
            >
              {t('grid.infiniteScroll.calculateTotal')}
            </button>
          ))}
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
