/**
 * Foreign key peek tooltip component
 * Displays a preview of related data when hovering over a foreign key cell
 */

import { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2, Link2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { TooltipRoot, TooltipTrigger, TooltipContent } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { Value, Namespace, ForeignKey, RelationFilter } from '@/lib/tauri';
import { formatValue } from './utils/dataGridUtils';
import { PeekState, MAX_PEEK_ROWS, MAX_PEEK_COLUMNS } from './hooks/useForeignKeyPeek';

export interface ForeignKeyPeekTooltipProps {
  children: ReactNode;
  peekKey: string;
  peekState?: PeekState;
  foreignKey: ForeignKey;
  relationLabel: string;
  referencedNamespace: Namespace | null;
  hasMultipleRelations: boolean;
  value: Value;
  onOpenChange?: (open: boolean) => void;
  onOpenRelatedTable?: (ns: Namespace, table: string, filter?: RelationFilter) => void;
}

export function ForeignKeyPeekTooltip({
  children,
  peekKey,
  peekState,
  foreignKey,
  relationLabel,
  referencedNamespace,
  hasMultipleRelations,
  value,
  onOpenChange,
  onOpenRelatedTable,
}: ForeignKeyPeekTooltipProps) {
  const { t } = useTranslation();

  const previewColumns = peekState?.result?.columns?.slice(0, MAX_PEEK_COLUMNS) ?? [];
  const previewRows = peekState?.result?.rows?.slice(0, MAX_PEEK_ROWS) ?? [];
  const extraColumns =
    peekState?.result?.columns && peekState.result.columns.length > MAX_PEEK_COLUMNS
      ? peekState.result.columns.length - MAX_PEEK_COLUMNS
      : 0;
  const extraRows =
    peekState?.result?.rows && peekState.result.rows.length > MAX_PEEK_ROWS
      ? peekState.result.rows.length - MAX_PEEK_ROWS
      : 0;

  return (
    <TooltipRoot delayDuration={300} disableHoverableContent={false} onOpenChange={onOpenChange}>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      <TooltipContent side="right" align="start" className="max-h-80 overflow-auto p-3 text-xs">
        <div className="flex items-start justify-between gap-2">
          <div>
            <div className="text-xs uppercase tracking-wide text-muted-foreground">
              {t('grid.peekTitle', { defaultValue: 'Relation' })}
            </div>
            <div className="text-sm font-medium text-foreground">{relationLabel}</div>
            {hasMultipleRelations && (
              <div className="text-xs text-muted-foreground">
                {t('grid.peekMultiple', { defaultValue: 'Multiple relations detected' })}
              </div>
            )}
            {foreignKey.constraint_name && (
              <div className="text-xs text-muted-foreground">{foreignKey.constraint_name}</div>
            )}
          </div>
          {onOpenRelatedTable && referencedNamespace && (
            <Button
              variant="link"
              size="sm"
              className="h-auto px-0 text-xs"
              onClick={event => {
                event.preventDefault();
                event.stopPropagation();
                onOpenRelatedTable(referencedNamespace, foreignKey.referenced_table, {
                  foreignKey,
                  value,
                });
              }}
            >
              <Link2 size={12} />
              {t('grid.openRelatedTable', { defaultValue: 'Open table' })}
            </Button>
          )}
        </div>
        <div className="mt-3 border-t border-border pt-3">
          {peekState?.status === 'error' ? (
            <div className="text-xs text-error">
              {peekState.error || t('grid.peekFailed', { defaultValue: 'Preview unavailable' })}
            </div>
          ) : !peekState || peekState.status === 'loading' ? (
            <div className="flex items-center gap-2 text-muted-foreground text-xs">
              <Loader2 size={14} className="animate-spin" />
              {t('grid.peekLoading', { defaultValue: 'Loading preview...' })}
            </div>
          ) : previewRows.length === 0 ? (
            <div className="text-xs text-muted-foreground">
              {t('grid.peekEmpty', { defaultValue: 'No matching row found' })}
            </div>
          ) : (
            <div className="space-y-3">
              {previewRows.map((row, rowIndex) => (
                <div key={`${peekKey}-row-${rowIndex}`} className="space-y-1">
                  <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1.5fr)] gap-x-3 gap-y-1">
                    {previewColumns.map((col, colIndex) => {
                      const rawValue = row.values[colIndex];
                      const displayValue = formatValue(rawValue);
                      return (
                        <div key={`${peekKey}-${rowIndex}-${col.name}`} className="contents">
                          <div className="text-xs text-muted-foreground truncate">{col.name}</div>
                          <div
                            className={cn(
                              'text-xs font-mono text-foreground truncate',
                              rawValue === null && 'italic text-muted-foreground'
                            )}
                          >
                            {displayValue}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                  {rowIndex === 0 && extraColumns > 0 && (
                    <div className="text-xs text-muted-foreground">
                      {t('grid.peekColumnsMore', {
                        defaultValue: '+{{count}} more columns',
                        count: extraColumns,
                      })}
                    </div>
                  )}
                </div>
              ))}
              {extraRows > 0 && (
                <div className="text-xs text-muted-foreground">
                  {t('grid.peekRowsMore', {
                    defaultValue: '+{{count}} more rows',
                    count: extraRows,
                  })}
                </div>
              )}
            </div>
          )}
        </div>
      </TooltipContent>
    </TooltipRoot>
  );
}
