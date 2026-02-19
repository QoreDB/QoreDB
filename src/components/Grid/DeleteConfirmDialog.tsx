// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import type { Value } from '@/lib/tauri';
import { formatValue } from './utils/dataGridUtils';

interface PreviewRow {
  index: number;
  values: { key: string; value: Value }[];
  hasMissing: boolean;
}

interface DeleteConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selectedCount: number;
  previewRows: PreviewRow[];
  totalSelectedRows: number;
  requiresConfirm: boolean;
  confirmLabel: string;
  confirmValue: string;
  onConfirmValueChange: (value: string) => void;
  onConfirm: () => void;
  isDeleting: boolean;
}

export function DeleteConfirmDialog({
  open,
  onOpenChange,
  selectedCount,
  previewRows,
  totalSelectedRows,
  requiresConfirm,
  confirmLabel,
  confirmValue,
  onConfirmValueChange,
  onConfirm,
  isDeleting,
}: DeleteConfirmDialogProps) {
  const { t } = useTranslation();

  const confirmReady = !requiresConfirm || confirmValue.trim() === confirmLabel;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t('grid.deleteTitle', { count: selectedCount })}</DialogTitle>
        </DialogHeader>

        <div className="space-y-3 text-sm">
          <p className="text-muted-foreground">
            {t('grid.confirmDelete', { count: selectedCount })}
          </p>

          {previewRows.length > 0 && (
            <div className="border border-border rounded-md bg-muted/20 p-2">
              <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-2">
                {t('grid.preview')}
              </div>
              <div className="space-y-1 text-xs">
                {previewRows.map(row => (
                  <div key={row.index} className="flex items-center justify-between gap-2">
                    <span className="text-muted-foreground">#{row.index}</span>
                    {row.hasMissing ? (
                      <span className="text-error">{t('grid.previewMissingPk')}</span>
                    ) : (
                      <span className="font-mono text-foreground">
                        {row.values
                          .map(entry => `${entry.key}=${formatValue(entry.value)}`)
                          .join(', ')}
                      </span>
                    )}
                  </div>
                ))}
                {totalSelectedRows > previewRows.length && (
                  <div className="text-muted-foreground">
                    {t('grid.previewMore', { count: totalSelectedRows - previewRows.length })}
                  </div>
                )}
              </div>
            </div>
          )}

          {requiresConfirm && (
            <div className="space-y-2">
              <label className="text-xs font-medium">
                {t('environment.confirmMessage', { name: confirmLabel })}
              </label>
              <Input
                value={confirmValue}
                onChange={event => onConfirmValueChange(event.target.value)}
                placeholder={confirmLabel}
              />
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={isDeleting}>
            {t('common.cancel')}
          </Button>
          <Button variant="destructive" onClick={onConfirm} disabled={!confirmReady || isDeleting}>
            {t('common.confirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
