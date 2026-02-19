// SPDX-License-Identifier: Apache-2.0

/**
 * Header bar component for DataGrid
 * Displays row counts, timing info, load more controls, and delete button
 */

import { Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import type { QueryResult } from '@/lib/tauri';

export interface DataGridHeaderProps {
  selectedCount: number;
  totalRows: number;
  result: QueryResult | null;
  canDelete: boolean;
  deleteDisabled: boolean;
  isDeleting: boolean;
  onDelete: () => void;
  readOnly: boolean;
  mutationsSupported: boolean;
}

export function DataGridHeader({
  selectedCount,
  totalRows,

  result,
  canDelete,
  deleteDisabled,
  isDeleting,
  onDelete,
  readOnly,
  mutationsSupported,
}: DataGridHeaderProps) {
  const { t } = useTranslation();

  return (
    <div className="text-xs text-muted-foreground flex items-center gap-3">
      {selectedCount > 0 ? (
        <>
          <span>{t('grid.rowsSelected', { count: selectedCount })}</span>
        </>
      ) : (
        <div className="flex items-center gap-3">
          <span>{t('grid.rowsTotal', { count: totalRows })}</span>

          {result && typeof result.execution_time_ms === 'number' && (
            <div className="flex items-center gap-2 border-l border-border pl-3 ml-1">
              <span title={t('query.time.execTooltip')}>
                {t('query.time.exec')}:{' '}
                <span className="font-mono text-foreground font-medium">
                  {result.execution_time_ms.toFixed(2)}ms
                </span>
              </span>
              {result.total_time_ms !== undefined && (
                <>
                  <span className="text-border/50">|</span>
                  <span title={t('query.time.totalTooltip')}>
                    {t('query.time.total')}:{' '}
                    <span className="font-mono text-foreground font-bold">
                      {result.total_time_ms.toFixed(2)}ms
                    </span>
                  </span>
                </>
              )}
            </div>
          )}
        </div>
      )}

      {canDelete && (
        <Button
          variant="destructive"
          size="sm"
          className="h-6 px-2 text-xs"
          onClick={onDelete}
          disabled={deleteDisabled}
          title={
            readOnly
              ? t('environment.blocked')
              : !mutationsSupported
                ? t('grid.mutationsNotSupported')
                : undefined
          }
        >
          <Trash2 size={12} className="mr-1" />
          {isDeleting ? t('grid.deleting') : t('grid.delete')}
        </Button>
      )}
    </div>
  );
}
