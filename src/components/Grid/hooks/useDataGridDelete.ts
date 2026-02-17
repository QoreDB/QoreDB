// SPDX-License-Identifier: Apache-2.0

/**
 * Hook for row deletion functionality in DataGrid
 * Manages delete operations with confirmation dialogs
 */

import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Table } from '@tanstack/react-table';
import { Value, Namespace, Environment, deleteRow, RowData as TauriRowData } from '@/lib/tauri';
import { RowData } from '../utils/dataGridUtils';

export interface UseDataGridDeleteProps {
  table: Table<RowData>;
  sessionId?: string;
  namespace?: Namespace;
  tableName?: string;
  primaryKey?: string[];
  environment?: Environment;
  readOnly?: boolean;
  mutationsSupported?: boolean;
  sandboxMode?: boolean;
  onSandboxDelete?: (pk: Record<string, Value>, oldValues: Record<string, Value>) => void;
  onRowsDeleted?: () => void;
}

export interface UseDataGridDeleteReturn {
  isDeleting: boolean;
  deleteDialogOpen: boolean;
  setDeleteDialogOpen: (open: boolean) => void;
  deleteConfirmValue: string;
  setDeleteConfirmValue: (value: string) => void;
  handleDelete: () => void;
  performDelete: (acknowledgedDangerous?: boolean) => Promise<void>;
  canDelete: boolean;
  deleteDisabled: boolean;
  deleteRequiresConfirm: boolean;
  previewRows: {
    index: number;
    values: { key: string; value: Value }[];
    hasMissing: boolean;
  }[];
}

/**
 * Hook for managing row deletion in the data grid
 */
export function useDataGridDelete({
  table,
  sessionId,
  namespace,
  tableName,
  primaryKey,
  environment = 'development',
  readOnly = false,
  mutationsSupported = true,
  sandboxMode = false,
  onSandboxDelete,
  onRowsDeleted,
}: UseDataGridDeleteProps): UseDataGridDeleteReturn {
  const { t } = useTranslation();
  const [isDeleting, setIsDeleting] = useState(false);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState('');

  const selectedRows = table.getSelectedRowModel().rows;
  const selectedCount = selectedRows.length;

  // Computed values
  const canDelete = Boolean(
    sessionId && namespace && tableName && primaryKey && primaryKey.length > 0 && selectedCount > 0
  );
  const deleteDisabled = selectedCount === 0 || isDeleting || readOnly || !mutationsSupported;
  const deleteRequiresConfirm = environment === 'production';

  // Preview rows for delete confirmation dialog
  const previewRows = selectedRows.slice(0, 10).map((row, index) => {
    const values =
      primaryKey?.map(pk => ({
        key: pk,
        value: row.original[pk],
      })) || [];
    return {
      index: index + 1,
      values,
      hasMissing: values.some(entry => entry.value === undefined),
    };
  });

  // Perform the actual delete operation
  const performDelete = useCallback(
    async (acknowledgedDangerous = false) => {
      if (!namespace || !tableName || !primaryKey || primaryKey.length === 0) return;

      const rowsToDelete = table.getSelectedRowModel().rows;
      if (rowsToDelete.length === 0) return;

      if (readOnly) {
        toast.error(t('environment.blocked'));
        return;
      }
      if (!mutationsSupported && !sandboxMode) {
        toast.error(t('grid.mutationsNotSupported'));
        return;
      }

      // Sandbox mode: add changes locally instead of executing
      if (sandboxMode && onSandboxDelete) {
        for (const row of rowsToDelete) {
          const pkData: Record<string, Value> = {};
          const oldValues: Record<string, Value> = {};
          let missingPk = false;

          for (const key of primaryKey) {
            if (row.original[key] === undefined) {
              missingPk = true;
              break;
            }
            pkData[key] = row.original[key];
          }

          if (missingPk) continue;

          // Capture all values for oldValues
          for (const [key, val] of Object.entries(row.original)) {
            oldValues[key] = val;
          }

          onSandboxDelete(pkData, oldValues);
        }
        table.resetRowSelection();
        return;
      }

      // Real deletion
      if (!sessionId) return;

      setIsDeleting(true);
      let successCount = 0;
      let failCount = 0;

      for (const row of rowsToDelete) {
        const pkData: TauriRowData = { columns: {} };
        let missingPk = false;

        for (const key of primaryKey) {
          if (row.original[key] === undefined) {
            missingPk = true;
            break;
          }
          pkData.columns[key] = row.original[key];
        }

        if (missingPk) {
          failCount++;
          continue;
        }

        try {
          const res = await deleteRow(
            sessionId,
            namespace.database,
            namespace.schema,
            tableName,
            pkData,
            acknowledgedDangerous
          );
          if (res.success) {
            successCount++;
          } else {
            failCount++;
          }
        } catch {
          failCount++;
        }
      }

      setIsDeleting(false);
      table.resetRowSelection();

      if (successCount > 0) {
        toast.success(t('grid.deleteSuccess', { count: successCount }));
        onRowsDeleted?.();
      }
      if (failCount > 0) {
        toast.error(t('grid.deleteError'));
      }
    },
    [
      table,
      sessionId,
      namespace,
      tableName,
      primaryKey,
      readOnly,
      mutationsSupported,
      sandboxMode,
      onSandboxDelete,
      onRowsDeleted,
      t,
    ]
  );

  // Handle delete button click (open confirmation dialog)
  const handleDelete = useCallback(() => {
    const rowsToDelete = table.getSelectedRowModel().rows;
    if (rowsToDelete.length === 0) return;

    if (readOnly) {
      toast.error(t('environment.blocked'));
      return;
    }
    if (!mutationsSupported && !sandboxMode) {
      toast.error(t('grid.mutationsNotSupported'));
      return;
    }

    setDeleteConfirmValue('');
    setDeleteDialogOpen(true);
  }, [table, readOnly, mutationsSupported, sandboxMode, t]);

  return {
    isDeleting,
    deleteDialogOpen,
    setDeleteDialogOpen,
    deleteConfirmValue,
    setDeleteConfirmValue,
    handleDelete,
    performDelete,
    canDelete,
    deleteDisabled,
    deleteRequiresConfirm,
    previewRows,
  };
}
