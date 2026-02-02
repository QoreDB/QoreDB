import { Table, PaginationState } from '@tanstack/react-table';
import { useTranslation } from 'react-i18next';
import { ChevronFirst, ChevronLast, ChevronLeft, ChevronRight } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { RowData } from './utils/dataGridUtils';

interface DataGridPaginationProps {
  table?: Table<RowData> | null;
  pagination: PaginationState;
  // Server-side pagination props
  serverSideTotalRows?: number;
  serverSidePage?: number;
  serverSidePageSize?: number;
  onServerPageChange?: (page: number) => void;
  onServerPageSizeChange?: (pageSize: number) => void;
}

const PAGE_SIZES = [25, 50, 100, 250];

export function DataGridPagination({ 
  table, 
  pagination,
  serverSideTotalRows,
  serverSidePage,
  serverSidePageSize,
  onServerPageChange,
  onServerPageSizeChange,
}: DataGridPaginationProps) {
  const { t } = useTranslation();
  
  // Calculate server-side pagination info
  const isServerSide = serverSideTotalRows !== undefined;
  const totalRows = isServerSide ? serverSideTotalRows : (table?.getFilteredRowModel().rows.length ?? 0);
  const effectivePageSize = isServerSide && serverSidePageSize ? serverSidePageSize : pagination.pageSize;
  const pageCount = isServerSide 
    ? Math.ceil(serverSideTotalRows / effectivePageSize) 
    : (table?.getPageCount() || 1);
  const currentPage = isServerSide && serverSidePage ? serverSidePage : pagination.pageIndex + 1;
  
  const canPreviousPage = isServerSide ? currentPage > 1 : (table?.getCanPreviousPage() ?? false);
  const canNextPage = isServerSide ? currentPage < pageCount : (table?.getCanNextPage() ?? false);

  const handlePageSizeChange = (newPageSize: number) => {
    if (isServerSide && onServerPageSizeChange) {
      onServerPageSizeChange(newPageSize);
    } else if (table) {
      table.setPageSize(newPageSize);
    }
  };

  const handleFirstPage = () => {
    if (isServerSide && onServerPageChange) {
      onServerPageChange(1);
    } else if (table) {
      table.setPageIndex(0);
    }
  };

  const handlePreviousPage = () => {
    if (isServerSide && onServerPageChange) {
      onServerPageChange(currentPage - 1);
    } else if (table) {
      table.previousPage();
    }
  };

  const handleNextPage = () => {
    if (isServerSide && onServerPageChange) {
      onServerPageChange(currentPage + 1);
    } else if (table) {
      table.nextPage();
    }
  };

  const handleLastPage = () => {
    if (isServerSide && onServerPageChange) {
      onServerPageChange(pageCount);
    } else if (table) {
      table.setPageIndex(pageCount - 1);
    }
  };

  return (
    <div className="flex items-center justify-between px-2 py-1 border-t border-border bg-muted/20">
      <div className="flex items-center gap-4 text-xs text-muted-foreground">
        <div className="flex items-center gap-2">
          <span>{t('grid.rowsPerPage')}:</span>
          <select
            value={effectivePageSize}
            onChange={e => handlePageSizeChange(Number(e.target.value))}
            className="h-7 px-2 rounded border border-border bg-background text-foreground text-xs focus:outline-none focus:ring-1 focus:ring-accent"
          >
            {PAGE_SIZES.map(size => (
              <option key={size} value={size}>
                {size}
              </option>
            ))}
          </select>
        </div>
        {isServerSide && (
          <span className="text-muted-foreground/70">
            {totalRows.toLocaleString()} {t('grid.totalRows')}
          </span>
        )}
      </div>

      <div className="flex items-center gap-1">
        <span className="text-xs text-muted-foreground mr-2">
          {t('grid.page')} {currentPage} {t('grid.of')} {pageCount}
        </span>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 w-7 p-0"
          onClick={handleFirstPage}
          disabled={!canPreviousPage}
          title={t('grid.firstPage')}
        >
          <ChevronFirst size={14} />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 w-7 p-0"
          onClick={handlePreviousPage}
          disabled={!canPreviousPage}
          title={t('grid.previousPage')}
        >
          <ChevronLeft size={14} />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 w-7 p-0"
          onClick={handleNextPage}
          disabled={!canNextPage}
          title={t('grid.nextPage')}
        >
          <ChevronRight size={14} />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 w-7 p-0"
          onClick={handleLastPage}
          disabled={!canNextPage}
          title={t('grid.lastPage')}
        >
          <ChevronLast size={14} />
        </Button>
      </div>
    </div>
  );
}
