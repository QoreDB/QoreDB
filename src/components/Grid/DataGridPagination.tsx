/**
 * DataGrid Pagination - Footer with page size and navigation
 */

import { Table, PaginationState } from '@tanstack/react-table';
import { useTranslation } from 'react-i18next';
import { ChevronFirst, ChevronLast, ChevronLeft, ChevronRight } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { RowData } from './utils/dataGridUtils';

interface DataGridPaginationProps {
  table: Table<RowData>;
  pagination: PaginationState;
}

const PAGE_SIZES = [25, 50, 100, 250];

export function DataGridPagination({ table, pagination }: DataGridPaginationProps) {
  const { t } = useTranslation();

  return (
    <div className="flex items-center justify-between px-2 py-1 border-t border-border bg-muted/20">
      <div className="flex items-center gap-4 text-xs text-muted-foreground">
        <div className="flex items-center gap-2">
          <span>{t('grid.rowsPerPage')}:</span>
          <select
            value={pagination.pageSize}
            onChange={e => table.setPageSize(Number(e.target.value))}
            className="h-7 px-2 rounded border border-border bg-background text-foreground text-xs focus:outline-none focus:ring-1 focus:ring-accent"
          >
            {PAGE_SIZES.map(size => (
              <option key={size} value={size}>{size}</option>
            ))}
          </select>
        </div>
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
  );
}
