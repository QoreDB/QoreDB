/**
 * Column header component for DataGrid
 * Displays column name with primary key/foreign key/index/unique indicators and sort controls
 */

import { Column } from '@tanstack/react-table';
import { useTranslation } from 'react-i18next';
import { ArrowUpDown, ArrowUp, ArrowDown, Link2, KeyRound, Zap, Fingerprint } from 'lucide-react';
import { TooltipRoot, TooltipTrigger, TooltipContent } from '@/components/ui/tooltip';
import { Value } from '@/lib/tauri';
import { RowData } from './utils/dataGridUtils';

export interface DataGridColumnHeaderProps {
  column: Column<RowData, Value>;
  columnName: string;
  isPrimaryKey: boolean;
  isForeignKey: boolean;
  fkTable?: string;
  isIndexed?: boolean;
  isUnique?: boolean;
  indexName?: string;
  isCompositeIndex?: boolean;
}

export function DataGridColumnHeader({
  column,
  columnName,
  isPrimaryKey,
  isForeignKey,
  fkTable,
  isIndexed = false,
  isUnique = false,
  indexName,
  isCompositeIndex = false,
}: DataGridColumnHeaderProps) {
  const { t } = useTranslation();

  return (
    <button
      className="flex items-center gap-1 hover:text-foreground transition-colors w-full text-left"
      onClick={() => column.toggleSorting()}
    >
      {isPrimaryKey && (
        <TooltipRoot>
          <TooltipTrigger asChild>
            <KeyRound size={12} className="shrink-0 text-accent" />
          </TooltipTrigger>
          <TooltipContent side="bottom" className="z-50">
            {t('grid.columnIndicators.primaryKey')}
          </TooltipContent>
        </TooltipRoot>
      )}
      {isForeignKey && (
        <TooltipRoot>
          <TooltipTrigger asChild>
            <Link2 size={12} className="shrink-0 text-info" />
          </TooltipTrigger>
          <TooltipContent side="bottom" className="max-w-xs">
            {t('grid.columnIndicators.foreignKey', { table: fkTable })}
          </TooltipContent>
        </TooltipRoot>
      )}
      {isUnique && !isPrimaryKey && (
        <TooltipRoot>
          <TooltipTrigger asChild>
            <Fingerprint size={12} className="shrink-0 text-warning" />
          </TooltipTrigger>
          <TooltipContent side="bottom" className="max-w-xs whitespace-nowrap">
            {t('grid.columnIndicators.unique')}
          </TooltipContent>
        </TooltipRoot>
      )}
      {isIndexed && !isUnique && !isPrimaryKey && (
        <TooltipRoot>
          <TooltipTrigger asChild>
            <Zap size={12} className="shrink-0 text-muted-foreground" />
          </TooltipTrigger>
          <TooltipContent side="bottom" className="max-w-xs">
            {isCompositeIndex
              ? t('grid.columnIndicators.indexComposite', { name: indexName })
              : t('grid.columnIndicators.indexed')}
          </TooltipContent>
        </TooltipRoot>
      )}
      <span className="truncate">{columnName}</span>
      {column.getIsSorted() === 'asc' ? (
        <ArrowUp size={14} className="shrink-0 text-accent" />
      ) : column.getIsSorted() === 'desc' ? (
        <ArrowDown size={14} className="shrink-0 text-accent" />
      ) : (
        <ArrowUpDown size={14} className="shrink-0 opacity-30" />
      )}
    </button>
  );
}
