// SPDX-License-Identifier: Apache-2.0

import { Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';

interface DataGridStatusBarProps {
  loadedRows: number;
  totalRows: number;
  isFetchingMore: boolean;
  isComplete: boolean;
}

export function DataGridStatusBar({
  loadedRows,
  totalRows,
  isFetchingMore,
  isComplete,
}: DataGridStatusBarProps) {
  const { t } = useTranslation();
  const percentage = totalRows > 0 ? Math.round((loadedRows / totalRows) * 100) : 100;

  return (
    <div className="flex items-center justify-between px-2 py-1 border-t border-border bg-muted/20">
      <div className="flex items-center gap-3 text-xs text-muted-foreground">
        <span>
          {isComplete
            ? t('grid.infiniteScroll.allLoaded', { total: totalRows.toLocaleString() })
            : t('grid.infiniteScroll.loaded', {
                loaded: loadedRows.toLocaleString(),
                total: totalRows.toLocaleString(),
              })}
        </span>
        {!isComplete && totalRows > 0 && (
          <div className="w-24 h-1.5 bg-muted rounded-full overflow-hidden">
            <div
              className="h-full bg-(--q-accent) rounded-full transition-all duration-300"
              style={{ width: `${percentage}%` }}
            />
          </div>
        )}
      </div>
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        {isFetchingMore && (
          <span className="flex items-center gap-1.5">
            <Loader2 size={12} className="animate-spin" />
            {t('grid.infiniteScroll.loading')}
          </span>
        )}
        {isComplete && totalRows > 0 && (
          <span className="text-muted-foreground/70">{t('grid.infiniteScroll.complete')}</span>
        )}
      </div>
    </div>
  );
}
