import { Column } from "@tanstack/react-table";
import { Input } from "@/components/ui/input";
import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { RowData } from "./utils/dataGridUtils";

interface GridColumnFilterProps {
  column: Column<RowData, unknown>;
}

export function GridColumnFilter({ column }: GridColumnFilterProps) {
  const { t } = useTranslation();
  const columnFilterValue = column.getFilterValue();
  const [value, setValue] = useState(columnFilterValue);

  // Sync internal state with column filter value
  useEffect(() => {
    setValue(columnFilterValue ?? "");
  }, [columnFilterValue]);

  // Debounce update
  useEffect(() => {
    const timeout = setTimeout(() => {
      column.setFilterValue(value);
    }, 500);

    return () => clearTimeout(timeout);
  }, [value, column]);

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
