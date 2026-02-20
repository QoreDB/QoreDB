// SPDX-License-Identifier: Apache-2.0

/**
 * Hook for clipboard copy functionality in DataGrid
 */

import type { Row } from '@tanstack/react-table';
import { useCallback, useState } from 'react';
import type { QueryResult } from '@/lib/tauri';
import { formatValue, type RowData } from '../utils/dataGridUtils';

interface UseDataGridCopyProps {
  rows: Row<RowData>[];
  getSelectedRows: () => Row<RowData>[];
  result: QueryResult | null;
  tableName?: string;
}

export function useDataGridCopy({
  rows,
  getSelectedRows,
  result,
  tableName,
}: UseDataGridCopyProps) {
  const [copied, setCopied] = useState<string | null>(null);

  const copyToClipboard = useCallback(
    async (format: 'csv' | 'json' | 'sql') => {
      const selectedRows = getSelectedRows();
      const rowsToCopy = selectedRows.length > 0 ? selectedRows : rows;

      if (rowsToCopy.length === 0) return;

      let content = '';
      const columnNames = result?.columns.map(c => c.name) || [];

      switch (format) {
        case 'csv': {
          const header = columnNames.join('\t');
          const dataRows = rowsToCopy.map(row =>
            columnNames
              .map(col => {
                const value = row.original[col];
                const formatted = formatValue(value);
                return formatted.replace(/[\t\n]/g, ' ');
              })
              .join('\t')
          );
          content = [header, ...dataRows].join('\n');
          break;
        }
        case 'json': {
          const jsonData = rowsToCopy.map(row => row.original);
          content = JSON.stringify(jsonData, null, 2);
          break;
        }
        case 'sql': {
          if (!result) return;
          const targetTable = tableName || 'table_name';
          const inserts = rowsToCopy.map(row => {
            const values = columnNames.map(col => {
              const value = row.original[col];
              if (value === null) return 'NULL';
              if (typeof value === 'number') return String(value);
              if (typeof value === 'boolean') return value ? 'TRUE' : 'FALSE';
              return `'${String(value).replace(/'/g, "''")}'`;
            });
            return `INSERT INTO ${targetTable} (${columnNames.join(', ')}) VALUES (${values.join(', ')});`;
          });
          content = inserts.join('\n');
          break;
        }
      }

      await navigator.clipboard.writeText(content);
      setCopied(format);
      setTimeout(() => setCopied(null), 2000);
    },
    [rows, getSelectedRows, result, tableName]
  );

  return { copyToClipboard, copied };
}
