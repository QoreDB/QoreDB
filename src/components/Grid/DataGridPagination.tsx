// SPDX-License-Identifier: Apache-2.0

import type { PaginationState, Table } from '@tanstack/react-table';
import { ChevronFirst, ChevronLast, ChevronLeft, ChevronRight } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { RowData } from './utils/dataGridUtils';

interface DataGridPaginationProps {
  table: Table<RowData>;
  pagination: PaginationState;
}

const PAGE_SIZES = [25, 50, 100, 250];

export function DataGridPagination({ table, pagination }: DataGridPaginationProps) {
  const { t } = useTranslation();

  const totalRows = table.getFilteredRowModel().rows.length;
  const pageCount = table.getPageCount() || 1;
  const currentPage = pagination.pageIndex + 1;
  const canPreviousPage = table.getCanPreviousPage();
  const canNextPage = table.getCanNextPage();

  return (
    <div className="flex items-center justify-between px-2 py-1 border-t border-border bg-muted/20">
      <div className="flex items-center gap-4 text-xs text-muted-foreground">
        <div className="flex items-center gap-2">
          <span>{t('grid.rowsPerPage')}:</span>
          <Select
            value={String(pagination.pageSize)}
            onValueChange={value => table.setPageSize(Number(value))}
          >
            <SelectTrigger className="h-7 w-[74px] text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {PAGE_SIZES.map(size => (
                <SelectItem key={size} value={String(size)}>
                  {size}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <span className="text-muted-foreground/70">
          {totalRows.toLocaleString()} {t('grid.totalRows')}
        </span>
      </div>

      <div className="flex items-center gap-1">
        <span className="text-xs text-muted-foreground mr-2">
          {t('grid.page')} {currentPage} {t('grid.of')} {pageCount}
        </span>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 w-7 p-0"
          onClick={() => table.setPageIndex(0)}
          disabled={!canPreviousPage}
          title={t('grid.firstPage')}
        >
          <ChevronFirst size={14} />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 w-7 p-0"
          onClick={() => table.previousPage()}
          disabled={!canPreviousPage}
          title={t('grid.previousPage')}
        >
          <ChevronLeft size={14} />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 w-7 p-0"
          onClick={() => table.nextPage()}
          disabled={!canNextPage}
          title={t('grid.nextPage')}
        >
          <ChevronRight size={14} />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 w-7 p-0"
          onClick={() => table.setPageIndex(pageCount - 1)}
          disabled={!canNextPage}
          title={t('grid.lastPage')}
        >
          <ChevronLast size={14} />
        </Button>
      </div>
    </div>
  );
}
