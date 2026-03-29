// SPDX-License-Identifier: Apache-2.0

/**
 * Virtualized table body component for DataGrid
 */

import { type Cell, flexRender, type Row } from '@tanstack/react-table';
import type { Virtualizer } from '@tanstack/react-virtual';
import { useTranslation } from 'react-i18next';
import { cn } from '@/lib/utils';
import type { RowData } from './utils/dataGridUtils';

/** Compute the left offset for a pinned cell by summing widths of all pinned columns before it. */
function getPinnedCellLeftOffset(cell: Cell<RowData, unknown>): number {
  const pinnedIds = cell.getContext().table.getState().columnPinning.left ?? [];
  let offset = 0;
  for (const id of pinnedIds) {
    if (id === cell.column.id) break;
    offset += cell.getContext().table.getColumn(id)?.getSize() ?? 0;
  }
  return offset;
}

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

        const isEven = virtualRow.index % 2 === 0;

        return (
          <tr
            key={row.id}
            className={cn(
              'border-b border-border hover:bg-muted/50 transition-colors',
              isEven && 'bg-muted/20',
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
              const isPinned = cell.column.getIsPinned();

              const tdStyle: React.CSSProperties = { width: cell.column.getSize() };
              if (isPinned === 'left') {
                tdStyle.position = 'sticky';
                tdStyle.left = getPinnedCellLeftOffset(cell);
                tdStyle.zIndex = 1;
              }

              return (
                <td
                  key={cell.id}
                  className={cn(
                    'px-3 py-1.5 max-w-xs [contain:content]',
                    isCellModified && !isInserted && !isDeleted && 'bg-warning/20',
                    isPinned === 'left' && 'bg-background shadow-[2px_0_4px_-2px_rgba(0,0,0,0.1)]'
                  )}
                  style={tdStyle}
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
