// SPDX-License-Identifier: Apache-2.0

import { useVirtualizer } from '@tanstack/react-virtual';
import { Binary, Check, SearchX } from 'lucide-react';
import { useCallback, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { BlobViewer } from '@/components/Grid/BlobViewer';
import { estimateByteSizeFromBase64, formatFileSize, isBinaryType } from '@/lib/binaryUtils';
import { cn } from '@/lib/utils';
import type { QueryResult } from '../../lib/tauri';

interface ResultsTableProps {
  result: QueryResult | null;
  height?: number;
}

function formatValue(value: unknown, dataType?: string): string {
  if (value === null) return 'NULL';
  if (value === undefined) return '';
  if (dataType && isBinaryType(dataType) && typeof value === 'string' && value.length > 0) {
    const size = estimateByteSizeFromBase64(value);
    return `<binary ${formatFileSize(size)}>`;
  }
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
      <div className="flex flex-col items-center justify-center gap-2 p-8 text-sm border rounded-md border-dashed">
        <SearchX size={24} className="text-muted-foreground/50" />
        <p className="text-muted-foreground">{t('results.noResults')}</p>
        <p className="text-xs text-muted-foreground/70">{t('results.noResultsHint')}</p>
      </div>
    );
  }

  const { columns, rows } = result;

  const parentRef = useRef<HTMLDivElement>(null);
  const headerRef = useRef<HTMLDivElement>(null);

  // Sync horizontal scroll between header and body
  const handleBodyScroll = useCallback(() => {
    if (parentRef.current && headerRef.current) {
      headerRef.current.scrollLeft = parentRef.current.scrollLeft;
    }
  }, []);

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 32,
    overscan: 10,
  });

  return (
    <div
      className="flex flex-col h-full border border-border rounded-md overflow-hidden bg-background"
      style={{ height }}
    >
      <div
        ref={headerRef}
        className="flex items-center bg-muted/50 border-b border-border h-[36px] shrink-0 overflow-x-hidden"
      >
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

      <div
        ref={parentRef}
        className="flex-1 overflow-auto bg-background"
        onScroll={handleBodyScroll}
      >
        <div
          style={{
            height: `${rowVirtualizer.getTotalSize()}px`,
            position: 'relative',
          }}
        >
          {rowVirtualizer.getVirtualItems().map(virtualRow => {
            const row = rows[virtualRow.index];
            return (
              <div
                key={virtualRow.index}
                className="flex items-center border-b border-border hover:bg-muted/30 transition-colors text-sm font-mono h-[32px]"
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  width: '100%',
                  transform: `translateY(${virtualRow.start}px)`,
                }}
              >
                {row.values.map((value: unknown, colIndex: number) => {
                  const col = columns[colIndex];
                  const isBinary =
                    col &&
                    isBinaryType(col.data_type) &&
                    typeof value === 'string' &&
                    value.length > 0;
                  return (
                    <ResultsTableCell
                      key={colIndex}
                      value={value}
                      dataType={col?.data_type}
                      columnName={col?.name ?? ''}
                      isBinary={isBinary}
                    />
                  );
                })}
              </div>
            );
          })}
        </div>
      </div>

      <div className="px-3 py-1 text-xs text-muted-foreground border-t border-border bg-muted/20 shrink-0">
        {t('results.rowCount', { count: rows.length })} •{' '}
        {t('results.timeMs', { time: result.execution_time_ms })}
      </div>
    </div>
  );
}

/** Cell component for ResultsTable that handles binary values with BlobViewer. */
function ResultsTableCell({
  value,
  dataType,
  columnName,
  isBinary,
}: {
  value: unknown;
  dataType?: string;
  columnName: string;
  isBinary: boolean;
}) {
  const [blobOpen, setBlobOpen] = useState(false);

  if (isBinary && typeof value === 'string') {
    return (
      <>
        <div
          className="flex-1 px-3 py-1 truncate border-r border-border last:border-r-0 h-full flex items-center gap-1.5 cursor-pointer hover:text-accent transition-colors"
          onClick={() => setBlobOpen(true)}
        >
          <Binary className="h-3 w-3 shrink-0 text-muted-foreground" />
          <span className="truncate text-muted-foreground italic text-xs">
            {formatValue(value, dataType)}
          </span>
        </div>
        <BlobViewer
          open={blobOpen}
          onOpenChange={setBlobOpen}
          value={value}
          columnName={columnName}
          dataType={dataType ?? ''}
        />
      </>
    );
  }

  return (
    <div
      className={cn(
        'flex-1 px-3 py-1 truncate border-r border-border last:border-r-0 h-full flex items-center',
        value === null && 'text-muted-foreground italic',
        typeof value === 'number' && 'text-right justify-end',
        typeof value === 'boolean' && 'text-center justify-center text-accent'
      )}
      title={String(value)}
    >
      {formatValue(value, dataType)}
    </div>
  );
}
