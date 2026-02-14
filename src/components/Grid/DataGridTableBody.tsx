/**
 * Virtualized table body component for DataGrid
 */

import { Row, flexRender } from '@tanstack/react-table';
import { Virtualizer } from '@tanstack/react-virtual';
import { useTranslation } from 'react-i18next';
import { cn } from '@/lib/utils';
import { RowData } from './utils/dataGridUtils';

interface RowMetadata {
  isInserted?: boolean;
  isDeleted?: boolean;
  isModified?: boolean;
  modifiedColumns: Set<string>;
}

export interface DataGridTableBodyProps {
  rows: Row<RowData>[];
  rowVirtualizer: Virtualizer<HTMLDivElement, Element>;
  rowMetadataMap: Map<number, RowMetadata>;
  sandboxMode: boolean;
  columnsCount: number;
}

export function DataGridTableBody({
  rows,
  rowVirtualizer,
  rowMetadataMap,
  sandboxMode,
  columnsCount,
}: DataGridTableBodyProps) {
  const { t } = useTranslation();
  const virtualItems = rowVirtualizer.getVirtualItems();

  if (virtualItems.length === 0) {
    return (
      <tbody>
        <tr>
          <td colSpan={columnsCount} className="text-center py-8 text-muted-foreground">
            {t('grid.noResults')}
          </td>
        </tr>
      </tbody>
    );
  }

  const firstItem = virtualItems[0];
  const lastItem = virtualItems[virtualItems.length - 1];

  return (
    <tbody>
      <tr style={{ height: `${firstItem?.start ?? 0}px` }} />
      {virtualItems.map(virtualRow => {
        const row = rows[virtualRow.index];
        const rowMeta = sandboxMode ? rowMetadataMap.get(virtualRow.index) : undefined;
        const isInserted = rowMeta?.isInserted ?? false;
        const isDeleted = rowMeta?.isDeleted ?? false;
        const isModified = rowMeta?.isModified ?? false;

        return (
          <tr
            key={row.id}
            className={cn(
              'border-b border-border hover:bg-muted/50 transition-colors',
              row.getIsSelected() && 'bg-accent/10 border-l-2 border-l-accent',
              isInserted && 'bg-success/10 hover:bg-success/15 border-l-2 border-l-success',
              isDeleted &&
                'bg-error/10 hover:bg-error/15 line-through opacity-60 border-l-2 border-l-error',
              isModified &&
                !isInserted &&
                !isDeleted &&
                'bg-warning/5 hover:bg-warning/10 border-l-2 border-l-warning'
            )}
          >
            {row.getVisibleCells().map(cell => {
              const columnId = cell.column.id;
              const isCellModified = rowMeta?.modifiedColumns.has(columnId) ?? false;

              return (
                <td
                  key={cell.id}
                  className={cn(
                    'px-3 py-1.5 max-w-xs',
                    isCellModified && !isInserted && !isDeleted && 'bg-warning/20'
                  )}
                  style={{ width: cell.column.getSize() }}
                >
                  {isInserted && cell.column.id === '__select' && (
                    <span className="inline-flex items-center px-1.5 py-0.5 text-[9px] font-bold rounded bg-success text-success-foreground mr-1.5">
                      {t('sandbox.row.new')}
                    </span>
                  )}
                  {flexRender(cell.column.columnDef.cell, cell.getContext())}
                </td>
              );
            })}
          </tr>
        );
      })}
      <tr style={{ height: `${rowVirtualizer.getTotalSize() - (lastItem?.end ?? 0)}px` }} />
    </tbody>
  );
}
