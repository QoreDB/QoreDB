import { useMemo, useState, useCallback, useRef, useEffect } from 'react';
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  getPaginationRowModel,
  flexRender,
  createColumnHelper,
  SortingState,
  RowSelectionState,
  PaginationState,
  ColumnDef,
} from '@tanstack/react-table';
import { useVirtualizer } from '@tanstack/react-virtual';
import { QueryResult, Value } from '../../lib/tauri';
import { cn } from '@/lib/utils';
import { 
  ArrowUpDown, 
  ArrowUp, 
  ArrowDown, 
  Check,
  FileJson,
  FileSpreadsheet,
  Code2,
  ChevronFirst,
  ChevronLast,
  ChevronLeft,
  ChevronRight,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useTranslation } from 'react-i18next';

interface DataGridProps {
  result: QueryResult | null;
  height?: number;
}

type RowData = Record<string, Value>;

// Format a Value for display
function formatValue(value: Value): string {
  if (value === null) return 'NULL';
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  if (typeof value === 'number') return String(value);
  if (typeof value === 'string') return value;
  if (typeof value === 'object') {
    if (Array.isArray(value)) return JSON.stringify(value);
    return JSON.stringify(value);
  }
  return String(value);
}

// Convert QueryResult rows to RowData format
function convertToRowData(result: QueryResult): RowData[] {
  return result.rows.map(row => {
    const data: RowData = {};
    result.columns.forEach((col, idx) => {
      data[col.name] = row.values[idx];
    });
    return data;
  });
}

export function DataGrid({ result, height = 400 }: DataGridProps) {
  const { t } = useTranslation();
  const [sorting, setSorting] = useState<SortingState>([]);
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({});
  const [pagination, setPagination] = useState<PaginationState>({
    pageIndex: 0,
    pageSize: 50,
  });
  const [copied, setCopied] = useState<string | null>(null);
  
  const parentRef = useRef<HTMLDivElement>(null);

  // Convert data to row format
  const data = useMemo(() => {
    if (!result) return [];
    return convertToRowData(result);
  }, [result]);

  // Create columns dynamically from result
  const columns = useMemo<ColumnDef<RowData, Value>[]>(() => {
    if (!result || result.columns.length === 0) return [];
    
    const columnHelper = createColumnHelper<RowData>();
    
    // Selection column
    const selectColumn = columnHelper.display({
      id: 'select',
      header: ({ table }) => (
        <input
          type="checkbox"
          checked={table.getIsAllRowsSelected()}
          onChange={table.getToggleAllRowsSelectedHandler()}
          className="h-4 w-4 rounded border-border cursor-pointer"
        />
      ),
      cell: ({ row }) => (
        <input
          type="checkbox"
          checked={row.getIsSelected()}
          onChange={row.getToggleSelectedHandler()}
          className="h-4 w-4 rounded border-border cursor-pointer"
        />
      ),
      size: 40,
    });

    // Data columns
    const dataColumns = result.columns.map(col =>
      columnHelper.accessor(row => row[col.name], {
        id: col.name,
        header: ({ column }) => (
          <button
            className="flex items-center gap-1 hover:text-foreground transition-colors w-full text-left"
            onClick={() => column.toggleSorting()}
          >
            <span className="truncate">{col.name}</span>
            {column.getIsSorted() === 'asc' ? (
              <ArrowUp size={14} className="shrink-0 text-accent" />
            ) : column.getIsSorted() === 'desc' ? (
              <ArrowDown size={14} className="shrink-0 text-accent" />
            ) : (
              <ArrowUpDown size={14} className="shrink-0 opacity-30" />
            )}
          </button>
        ),
        cell: info => {
          const value = info.getValue();
          const formatted = formatValue(value);
          const isNull = value === null;
          return (
            <span className={cn(
              "truncate block",
              isNull && "text-muted-foreground italic"
            )}>
              {formatted}
            </span>
          );
        },
        sortingFn: (rowA, rowB, columnId) => {
          const a = rowA.getValue(columnId) as Value;
          const b = rowB.getValue(columnId) as Value;
          
          // Handle nulls
          if (a === null && b === null) return 0;
          if (a === null) return 1;
          if (b === null) return -1;
          
          // Compare by type
          if (typeof a === 'number' && typeof b === 'number') {
            return a - b;
          }
          return String(a).localeCompare(String(b));
        },
      })
    );

    return [selectColumn, ...dataColumns];
  }, [result]);

  const table = useReactTable({
    data,
    columns,
    state: { sorting, rowSelection, pagination },
    onSortingChange: setSorting,
    onRowSelectionChange: setRowSelection,
    onPaginationChange: setPagination,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    enableRowSelection: true,
  });

  const { rows } = table.getRowModel();

  // Virtual scrolling
  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 32,
    overscan: 10,
  });

  // Copy functionality
  const copyToClipboard = useCallback(async (format: 'csv' | 'json' | 'sql') => {
    const selectedRows = table.getSelectedRowModel().rows;
    const rowsToCopy = selectedRows.length > 0 ? selectedRows : rows;
    
    if (rowsToCopy.length === 0) return;

    let content = '';
    const columnNames = result?.columns.map(c => c.name) || [];

    switch (format) {
      case 'csv': {
        const header = columnNames.join('\t');
        const dataRows = rowsToCopy.map(row => 
          columnNames.map(col => {
            const value = row.original[col];
            const formatted = formatValue(value);
            // Escape tabs and newlines
            return formatted.replace(/[\t\n]/g, ' ');
          }).join('\t')
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
        const tableName = 'table_name'; // TODO: get from context
        const inserts = rowsToCopy.map(row => {
          const values = columnNames.map(col => {
            const value = row.original[col];
            if (value === null) return 'NULL';
            if (typeof value === 'number') return String(value);
            if (typeof value === 'boolean') return value ? 'TRUE' : 'FALSE';
            // Escape single quotes
            return `'${String(value).replace(/'/g, "''")}'`;
          });
          return `INSERT INTO ${tableName} (${columnNames.join(', ')}) VALUES (${values.join(', ')});`;
        });
        content = inserts.join('\n');
        break;
      }
    }

    await navigator.clipboard.writeText(content);
    setCopied(format);
    setTimeout(() => setCopied(null), 2000);
  }, [rows, table, result]);

  // Keyboard shortcuts
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === 'c') {
        e.preventDefault();
        copyToClipboard('csv');
      }
      if ((e.metaKey || e.ctrlKey) && e.key === 'a') {
        if (document.activeElement?.closest('[data-datagrid]')) {
          e.preventDefault();
          table.toggleAllRowsSelected(true);
        }
      }
    }
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [copyToClipboard, table]);

  if (!result || result.columns.length === 0) {
    return (
      <div className="flex items-center justify-center h-40 text-muted-foreground text-sm">
        {t('grid.noData')}
      </div>
    );
  }

  const selectedCount = Object.keys(rowSelection).length;

  return (
    <div className="flex flex-col gap-2" data-datagrid>
      {/* Toolbar */}
      <div className="flex items-center justify-between px-1">
        <div className="text-xs text-muted-foreground">
          {selectedCount > 0 ? (
            <span>{t('grid.rowsSelected', { count: selectedCount })}</span>
          ) : (
            <span>{t('grid.rowsTotal', { count: data.length })}</span>
          )}
        </div>
        <div className="flex items-center gap-1">
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs"
            onClick={() => copyToClipboard('csv')}
            title={t('grid.copyCSV')}
          >
            {copied === 'csv' ? <Check size={14} className="text-green-500" /> : <FileSpreadsheet size={14} />}
            <span className="ml-1">CSV</span>
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs"
            onClick={() => copyToClipboard('json')}
            title={t('grid.copyJSON')}
          >
            {copied === 'json' ? <Check size={14} className="text-green-500" /> : <FileJson size={14} />}
            <span className="ml-1">JSON</span>
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs"
            onClick={() => copyToClipboard('sql')}
            title={t('grid.copySQL')}
          >
            {copied === 'sql' ? <Check size={14} className="text-green-500" /> : <Code2 size={14} />}
            <span className="ml-1">SQL</span>
          </Button>
        </div>
      </div>

      {/* Table */}
      <div 
        ref={parentRef}
        className="border border-border rounded-md overflow-auto"
        style={{ height }}
      >
        <table className="w-full text-sm border-collapse">
          <thead className="sticky top-0 z-10 bg-muted/80 backdrop-blur-sm">
            {table.getHeaderGroups().map(headerGroup => (
              <tr key={headerGroup.id}>
                {headerGroup.headers.map(header => (
                  <th
                    key={header.id}
                    className="px-3 py-2 text-left font-medium text-muted-foreground border-b border-border"
                    style={{ width: header.getSize() }}
                  >
                    {header.isPlaceholder
                      ? null
                      : flexRender(header.column.columnDef.header, header.getContext())}
                  </th>
                ))}
              </tr>
            ))}
          </thead>
          <tbody>
            {rowVirtualizer.getVirtualItems().length === 0 ? (
              <tr>
                <td colSpan={columns.length} className="px-3 py-8 text-center text-muted-foreground">
                  {t('grid.noResults')}
                </td>
              </tr>
            ) : (
              <>
                {/* Spacer for virtual scroll */}
                {rowVirtualizer.getVirtualItems()[0]?.start > 0 && (
                  <tr style={{ height: rowVirtualizer.getVirtualItems()[0].start }} />
                )}
                {rowVirtualizer.getVirtualItems().map(virtualRow => {
                  const row = rows[virtualRow.index];
                  return (
                    <tr
                      key={row.id}
                      className={cn(
                        "border-b border-border/50 hover:bg-muted/30 transition-colors",
                        row.getIsSelected() && "bg-accent/10"
                      )}
                      onClick={() => row.toggleSelected()}
                    >
                      {row.getVisibleCells().map(cell => (
                        <td
                          key={cell.id}
                          className="px-3 py-1.5 font-mono text-xs"
                          style={{ maxWidth: 300 }}
                        >
                          {flexRender(cell.column.columnDef.cell, cell.getContext())}
                        </td>
                      ))}
                    </tr>
                  );
                })}
                {/* Bottom spacer */}
                {rowVirtualizer.getVirtualItems().length > 0 && (
                  <tr
                    style={{
                      height:
                        rowVirtualizer.getTotalSize() -
                        (rowVirtualizer.getVirtualItems()[rowVirtualizer.getVirtualItems().length - 1]?.end || 0),
                    }}
                  />
                )}
              </>
            )}
          </tbody>
        </table>
      </div>

      {/* Pagination */}
      <div className="flex items-center justify-between px-2 py-1 border-t border-border bg-muted/20">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span>{t('grid.rowsPerPage')}:</span>
          <select
            value={pagination.pageSize}
            onChange={e => table.setPageSize(Number(e.target.value))}
            className="h-7 px-2 rounded border border-border bg-background text-foreground text-xs focus:outline-none focus:ring-1 focus:ring-accent"
          >
            {[25, 50, 100, 250].map(size => (
              <option key={size} value={size}>{size}</option>
            ))}
          </select>
        </div>
        
        <div className="flex items-center gap-1">
          <span className="text-xs text-muted-foreground mr-2">
            {t('grid.page')} {pagination.pageIndex + 1} {t('grid.of')} {table.getPageCount() || 1}
          </span>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            onClick={() => table.setPageIndex(0)}
            disabled={!table.getCanPreviousPage()}
            title={t('grid.firstPage')}
          >
            <ChevronFirst size={14} />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            onClick={() => table.previousPage()}
            disabled={!table.getCanPreviousPage()}
            title={t('grid.previousPage')}
          >
            <ChevronLeft size={14} />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            onClick={() => table.nextPage()}
            disabled={!table.getCanNextPage()}
            title={t('grid.nextPage')}
          >
            <ChevronRight size={14} />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            onClick={() => table.setPageIndex(table.getPageCount() - 1)}
            disabled={!table.getCanNextPage()}
            title={t('grid.lastPage')}
          >
            <ChevronLast size={14} />
          </Button>
        </div>
      </div>
    </div>
  );
}
