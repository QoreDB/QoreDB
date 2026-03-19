// SPDX-License-Identifier: Apache-2.0
/** biome-ignore-all lint/a11y/useKeyWithClickEvents: header cells handle resize and context-menu pointer interactions */
/** biome-ignore-all lint/a11y/noStaticElementInteractions: header cells intentionally support mouse-only grid interactions */

/**
 * Table header component for DataGrid with resizable columns, filters, and column pinning
 */

import { flexRender, type Header, type Table } from '@tanstack/react-table';
import { Pin, PinOff } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { cn } from '@/lib/utils';
import { GridColumnFilter } from './GridColumnFilter';
import type { RowData } from './utils/dataGridUtils';

export interface DataGridTableHeaderProps {
  table: Table<RowData>;
  showFilters: boolean;
}

/** Compute the left offset for a pinned column by summing widths of all pinned columns before it. */
function getPinnedLeftOffset(
  header: Header<RowData, unknown>,
  headers: Header<RowData, unknown>[]
): number {
  const pinnedIds = header.getContext().table.getState().columnPinning.left ?? [];
  let offset = 0;
  for (const id of pinnedIds) {
    if (id === header.column.id) break;
    const pinnedHeader = headers.find(h => h.column.id === id);
    if (pinnedHeader) offset += pinnedHeader.getSize();
  }
  return offset;
}

export function DataGridTableHeader({ table, showFilters }: DataGridTableHeaderProps) {
  return (
    <thead className="sticky top-0 z-10 bg-muted shadow-sm">
      {table.getHeaderGroups().map(headerGroup => (
        <tr key={headerGroup.id}>
          {headerGroup.headers.map(header => (
            <DataGridTableHeaderCell
              key={header.id}
              header={header}
              headers={headerGroup.headers}
              showFilters={showFilters}
            />
          ))}
        </tr>
      ))}
    </thead>
  );
}

interface DataGridTableHeaderCellProps {
  header: Header<RowData, unknown>;
  headers: Header<RowData, unknown>[];
  showFilters: boolean;
}

function DataGridTableHeaderCell({ header, headers, showFilters }: DataGridTableHeaderCellProps) {
  const { t } = useTranslation();
  const isPinned = header.column.getIsPinned();
  const isDataColumn = header.column.id !== 'select' && header.column.id !== 'actions';

  const thStyle: React.CSSProperties = { width: header.getSize() };
  if (isPinned === 'left') {
    thStyle.position = 'sticky';
    thStyle.left = getPinnedLeftOffset(header, headers);
    thStyle.zIndex = 20;
  }

  const thContent = (
    <th
      className={cn(
        'px-3 py-2 text-left font-medium text-muted-foreground border-b border-border relative group',
        isPinned === 'left' && 'bg-muted shadow-[2px_0_4px_-2px_rgba(0,0,0,0.1)]'
      )}
      style={thStyle}
    >
      {header.isPlaceholder
        ? null
        : flexRender(header.column.columnDef.header, header.getContext())}
      {header.column.getCanResize() && (
        <div
          onMouseDown={header.getResizeHandler()}
          onTouchStart={header.getResizeHandler()}
          onDoubleClick={() => header.column.resetSize()}
          className={cn(
            'absolute right-0 top-0 h-full w-1 cursor-col-resize select-none touch-none',
            'opacity-0 group-hover:opacity-100 hover:bg-accent transition-opacity',
            header.column.getIsResizing() && 'bg-accent opacity-100'
          )}
        />
      )}
      {showFilters && header.column.getCanFilter() && (
        <div className="mt-2" onClick={e => e.stopPropagation()}>
          <GridColumnFilter column={header.column} />
        </div>
      )}
    </th>
  );

  if (!isDataColumn) return thContent;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{thContent}</ContextMenuTrigger>
      <ContextMenuContent>
        {isPinned ? (
          <ContextMenuItem onClick={() => header.column.pin(false)}>
            <PinOff size={14} />
            {t('grid.unpinColumn')}
          </ContextMenuItem>
        ) : (
          <ContextMenuItem onClick={() => header.column.pin('left')}>
            <Pin size={14} />
            {t('grid.pinColumnLeft')}
          </ContextMenuItem>
        )}
      </ContextMenuContent>
    </ContextMenu>
  );
}
