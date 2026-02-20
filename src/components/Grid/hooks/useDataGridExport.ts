// SPDX-License-Identifier: Apache-2.0

/**
 * Hook for file export functionality in DataGrid
 */

import type { Row } from '@tanstack/react-table';
import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import type { QueryResult } from '@/lib/tauri';
import { escapeCSV, formatValue, type RowData } from '../utils/dataGridUtils';

interface UseDataGridExportProps {
  rows: Row<RowData>[];
  getSelectedRows: () => Row<RowData>[];
  result: QueryResult | null;
  tableName?: string;
}

export function useDataGridExport({
  rows,
  getSelectedRows,
  result,
  tableName,
}: UseDataGridExportProps) {
  const { t } = useTranslation();

  const exportToFile = useCallback(
    async (format: 'csv' | 'json') => {
      const selectedRows = getSelectedRows();
      const rowsToExport = selectedRows.length > 0 ? selectedRows : rows;

      if (rowsToExport.length === 0) {
        toast.error(t('grid.noDataToExport'));
        return;
      }

      const columnNames = result?.columns.map(c => c.name) || [];
      let content = '';
      let extension = '';
      const defaultName = tableName || 'export';

      if (format === 'csv') {
        extension = 'csv';
        const header = columnNames.join(',');
        const dataRows = rowsToExport.map(row =>
          columnNames
            .map(col => {
              const value = row.original[col];
              const formatted = formatValue(value);
              return escapeCSV(formatted);
            })
            .join(',')
        );
        content = [header, ...dataRows].join('\n');
      } else {
        extension = 'json';
        const jsonData = rowsToExport.map(row => row.original);
        content = JSON.stringify(jsonData, null, 2);
      }

      try {
        const filePath = await save({
          defaultPath: `${defaultName}.${extension}`,
          filters: [
            {
              name: format.toUpperCase(),
              extensions: [extension],
            },
          ],
        });

        if (filePath) {
          await writeTextFile(filePath, content);
          toast.success(
            t('grid.exportSuccess', {
              count: rowsToExport.length,
              path: filePath.split(/[\\/]/).pop(),
            })
          );
        }
      } catch (err) {
        console.error('Export failed:', err);
        toast.error(t('grid.exportError'), {
          description: err instanceof Error ? err.message : String(err),
        });
      }
    },
    [rows, getSelectedRows, result, tableName, t]
  );

  return { exportToFile };
}
