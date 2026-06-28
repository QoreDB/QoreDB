// SPDX-License-Identifier: Apache-2.0

import { Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { BulkEditButton } from './BulkEditButton';
import { GridTimingInfo } from './GridTimingInfo';

export interface DataGridHeaderProps {
  selectedCount: number;
  totalRows: number;
  canDelete: boolean;
  deleteDisabled: boolean;
  isDeleting: boolean;
  onDelete: () => void;
  readOnly: boolean;
  mutationsSupported: boolean;
  canBulkEdit: boolean;
  bulkEditDisabled: boolean;
  bulkEditRequiresPro: boolean;
  onBulkEdit: () => void;
  execTimeMs?: number;
  totalTimeMs?: number;
}

export function DataGridHeader({
  selectedCount,
  totalRows,
  canDelete,
  deleteDisabled,
  isDeleting,
  onDelete,
  readOnly,
  mutationsSupported,
  canBulkEdit,
  bulkEditDisabled,
  bulkEditRequiresPro,
  onBulkEdit,
  execTimeMs,
  totalTimeMs,
}: DataGridHeaderProps) {
  const { t } = useTranslation();

  return (
    <div className="flex min-w-0 flex-1 items-center gap-3 overflow-hidden text-xs text-muted-foreground">
      {selectedCount > 0 ? (
        <span>{t('grid.rowsSelected', { count: selectedCount })}</span>
      ) : (
        <span>{t('grid.rowsTotal', { count: totalRows })}</span>
      )}

      {selectedCount === 0 && <GridTimingInfo execTimeMs={execTimeMs} totalTimeMs={totalTimeMs} />}

      {canBulkEdit && (
        <BulkEditButton
          selectedCount={selectedCount}
          disabled={bulkEditDisabled}
          requiresPro={bulkEditRequiresPro}
          readOnly={readOnly}
          mutationsSupported={mutationsSupported}
          onClick={onBulkEdit}
        />
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
