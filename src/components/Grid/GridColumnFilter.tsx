// SPDX-License-Identifier: Apache-2.0

import type { Column } from '@tanstack/react-table';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { FilterOperator } from '@/lib/tauri';
import type { RowData } from './utils/dataGridUtils';

export type GridFilterOp = Extract<FilterOperator, 'like' | 'regex' | 'text' | 'eq' | 'neq'>;

export interface GridColumnFilterValue {
  operator: GridFilterOp;
  value: string;
  regex_flags?: string;
  text_language?: string;
}

const OPERATORS: { value: GridFilterOp; labelKey: string }[] = [
  { value: 'like', labelKey: 'grid.filterOp.like' },
  { value: 'eq', labelKey: 'grid.filterOp.eq' },
  { value: 'neq', labelKey: 'grid.filterOp.neq' },
  { value: 'regex', labelKey: 'grid.filterOp.regex' },
  { value: 'text', labelKey: 'grid.filterOp.text' },
];

interface GridColumnFilterProps {
  column: Column<RowData, unknown>;
}

function normalizeFilterValue(raw: unknown): GridColumnFilterValue {
  if (raw && typeof raw === 'object' && 'operator' in raw) {
    const obj = raw as Partial<GridColumnFilterValue>;
    return {
      operator: (obj.operator as GridFilterOp) ?? 'like',
      value: obj.value ?? '',
      regex_flags: obj.regex_flags,
      text_language: obj.text_language,
    };
  }
  // Legacy: plain string
  return { operator: 'like', value: (raw as string) ?? '' };
}

export function GridColumnFilter({ column }: GridColumnFilterProps) {
  const { t } = useTranslation();
  const columnFilterValue = column.getFilterValue();
  const initial = useMemo(() => normalizeFilterValue(columnFilterValue), [columnFilterValue]);
  const [operator, setOperator] = useState<GridFilterOp>(initial.operator);
  const [value, setValue] = useState<string>(initial.value);
  const [flags, setFlags] = useState<string>(initial.regex_flags ?? '');
  const columnRef = useRef(column);
  columnRef.current = column;

  useEffect(() => {
    const norm = normalizeFilterValue(columnFilterValue);
    setOperator(norm.operator);
    setValue(norm.value);
    setFlags(norm.regex_flags ?? '');
  }, [columnFilterValue]);

  useEffect(() => {
    const timeout = setTimeout(() => {
      if (!value) {
        columnRef.current.setFilterValue(undefined);
        return;
      }
      const payload: GridColumnFilterValue = {
        operator,
        value,
        ...(operator === 'regex' && flags ? { regex_flags: flags } : {}),
      };
      columnRef.current.setFilterValue(payload);
    }, 500);
    return () => clearTimeout(timeout);
  }, [operator, value, flags]);

  return (
    <div
      className="flex items-center gap-1 mt-1"
      onClick={e => e.stopPropagation()}
      onKeyDown={e => e.stopPropagation()}
    >
      <Select value={operator} onValueChange={v => setOperator(v as GridFilterOp)}>
        <SelectTrigger className="h-7 px-1.5 text-[10px] font-mono w-16 shrink-0">
          <SelectValue />
        </SelectTrigger>
        <SelectContent className="min-w-[8rem]">
          {OPERATORS.map(op => (
            <SelectItem key={op.value} value={op.value} className="text-xs">
              {t(op.labelKey)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Input
        className="h-7 flex-1 text-xs px-2 bg-background/50"
        placeholder={t(
          operator === 'regex'
            ? 'grid.filterPlaceholderRegex'
            : operator === 'text'
              ? 'grid.filterPlaceholderText'
              : 'grid.filterPlaceholder'
        )}
        value={value}
        onChange={e => setValue(e.target.value)}
      />
      {operator === 'regex' && (
        <Input
          className="h-7 w-10 text-[10px] px-1.5 bg-background/50 font-mono"
          placeholder="i"
          title={t('grid.filterRegexFlagsHint')}
          value={flags}
          onChange={e => setFlags(e.target.value.replace(/[^imxs]/g, ''))}
          maxLength={4}
        />
      )}
    </div>
  );
}
