// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from 'react-i18next';
import { QueryResult } from '../../lib/tauri';
import { cn } from '@/lib/utils';
import { Check } from 'lucide-react';

interface ResultsTableProps {
  result: QueryResult | null;
  height?: number;
}

function formatValue(value: unknown): string {
  if (value === null) return 'NULL';
  if (value === undefined) return '';
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

export function ResultsTable({ result, height = 400 }: ResultsTableProps) {
  const { t } = useTranslation();

  if (!result || result.columns.length === 0) {
    if (result?.affected_rows !== undefined) {
      return (
        <div className="flex items-center gap-2 p-4 text-sm text-success bg-success/10 border border-success/20 rounded-md">
          <Check size={16} />
          {t('results.affectedRows', {
            count: result.affected_rows,
            time: result.execution_time_ms,
          })}
        </div>
      );
    }
    return (
      <div className="flex items-center justify-center p-8 text-muted-foreground text-sm border rounded-md border-dashed">
        {t('results.noResults')}
      </div>
    );
  }

  const { columns, rows } = result;

  return (
    <div
      className="flex flex-col h-full border border-border rounded-md overflow-hidden bg-background"
      style={{ height }}
    >
      {/* Header */}
      <div className="flex items-center bg-muted/50 border-b border-border h-[36px] shrink-0">
        {columns.map((col, i) => (
          <div
            key={i}
            className="flex-1 px-3 py-2 text-xs font-semibold text-muted-foreground uppercase tracking-wider truncate border-r border-border last:border-r-0"
            title={col.data_type}
          >
            {col.name}
          </div>
        ))}
      </div>

      {/* Rows (Simple overflow, no virtualization for stability) */}
      <div className="flex-1 overflow-auto bg-background">
        {rows.map((row, rowIndex) => (
          <div
            key={rowIndex}
            className="flex items-center border-b border-border hover:bg-muted/30 transition-colors text-sm font-mono h-[32px]"
          >
            {row.values.map((value: unknown, colIndex: number) => (
              <div
                key={colIndex}
                className={cn(
                  'flex-1 px-3 py-1 truncate border-r border-border last:border-r-0 h-full flex items-center',
                  value === null && 'text-muted-foreground italic',
                  typeof value === 'number' && 'text-right justify-end',
                  typeof value === 'boolean' && 'text-center justify-center text-accent'
                )}
                title={String(value)}
              >
                {formatValue(value)}
              </div>
            ))}
          </div>
        ))}
      </div>

      {/* Footer */}
      <div className="px-3 py-1 text-xs text-muted-foreground border-t border-border bg-muted/20 shrink-0">
        {t('results.rowCount', { count: rows.length })} •{' '}
        {t('results.timeMs', { time: result.execution_time_ms })}
      </div>
    </div>
  );
}
