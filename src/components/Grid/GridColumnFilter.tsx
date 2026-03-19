// SPDX-License-Identifier: Apache-2.0

import type { Column } from '@tanstack/react-table';
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Input } from '@/components/ui/input';
import type { RowData } from './utils/dataGridUtils';

interface GridColumnFilterProps {
  column: Column<RowData, unknown>;
}

export function GridColumnFilter({ column }: GridColumnFilterProps) {
  const { t } = useTranslation();
  const columnFilterValue = column.getFilterValue();
  const [value, setValue] = useState(columnFilterValue);
  const columnRef = useRef(column);
  columnRef.current = column;

  // Sync internal state with column filter value
  useEffect(() => {
    setValue(columnFilterValue ?? '');
  }, [columnFilterValue]);

  // Debounce update - only re-trigger on value change, not on column reference change
  useEffect(() => {
    const timeout = setTimeout(() => {
      columnRef.current.setFilterValue(value);
    }, 500);

    return () => clearTimeout(timeout);
  }, [value]);

  return (
    <Input
      className="h-7 w-full text-xs px-2 mt-1 bg-background/50"
      placeholder={t('grid.filterPlaceholder')}
      value={(value as string) ?? ''}
      onChange={e => setValue(e.target.value)}
      onClick={e => e.stopPropagation()}
    />
  );
}
