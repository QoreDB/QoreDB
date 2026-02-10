/**
 * Table header component for DataGrid with resizable columns and filters
 */

import { Table, flexRender, Header } from '@tanstack/react-table';
import { cn } from '@/lib/utils';
import { GridColumnFilter } from './GridColumnFilter';
import { RowData } from './utils/dataGridUtils';

export interface DataGridTableHeaderProps {
  table: Table<RowData>;
  showFilters: boolean;
}

export function DataGridTableHeader({ table, showFilters }: DataGridTableHeaderProps) {
  return (
    <thead className="sticky top-0 z-10 bg-muted/80 backdrop-blur-sm shadow-sm">
      {table.getHeaderGroups().map(headerGroup => (
        <tr key={headerGroup.id}>
          {headerGroup.headers.map(header => (
            <DataGridTableHeaderCell key={header.id} header={header} showFilters={showFilters} />
          ))}
        </tr>
      ))}
    </thead>
  );
}

interface DataGridTableHeaderCellProps {
  header: Header<RowData, unknown>;
  showFilters: boolean;
}

function DataGridTableHeaderCell({ header, showFilters }: DataGridTableHeaderCellProps) {
  return (
    <th
      className="px-3 py-2 text-left font-medium text-muted-foreground border-b border-border relative group"
      style={{ width: header.getSize() }}
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
}
