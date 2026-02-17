// SPDX-License-Identifier: BUSL-1.1

/**
 * DiffResultsGrid - Virtualized grid for displaying diff results
 */
import { useRef, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useVirtualizer } from '@tanstack/react-virtual';
import { PlusCircle, MinusCircle, ArrowLeftRight, CheckCircle2, GitCompare } from 'lucide-react';
import { cn } from '@/lib/utils';
import { DiffResult, DiffRow, DiffRowStatus, formatDiffValue } from '@/lib/diffUtils';

interface DiffResultsGridProps {
  diffResult: DiffResult | null;
  filteredRows: DiffRow[];
}

const ROW_HEIGHT = 36;
const HEADER_HEIGHT = 40;

export function DiffResultsGrid({ diffResult, filteredRows }: DiffResultsGridProps) {
  const { t } = useTranslation();
  const parentRef = useRef<HTMLDivElement>(null);

  const rowVirtualizer = useVirtualizer({
    count: filteredRows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
  });

  if (!diffResult) {
    return (
      <div className="flex flex-col items-center justify-center h-64 text-muted-foreground">
        <GitCompare size={48} className="mb-4 opacity-50" />
        <p className="text-sm">{t('diff.noData')}</p>
        <p className="text-xs mt-1 opacity-70">{t('diff.noDataHint')}</p>
      </div>
    );
  }

  if (filteredRows.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-64 text-muted-foreground">
        <CheckCircle2 size={48} className="mb-4 opacity-50" />
        <p className="text-sm">{t('diff.noDifferences')}</p>
      </div>
    );
  }

  const { columns } = diffResult;

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Header */}
      <div
        className="flex items-center bg-muted/50 border-b border-border shrink-0"
        style={{ height: HEADER_HEIGHT }}
      >
        {/* Status column */}
        <div className="w-10 shrink-0 flex items-center justify-center border-r border-border">
          <span className="sr-only">Status</span>
        </div>
        {/* Data columns */}
        {columns.map((col, i) => (
          <div
            key={i}
            className="flex-1 min-w-[120px] px-3 py-2 text-xs font-semibold text-muted-foreground uppercase tracking-wider truncate border-r border-border last:border-r-0"
            title={`${col.name} (${col.data_type})`}
          >
            {col.name}
          </div>
        ))}
      </div>

      {/* Virtualized rows */}
      <div ref={parentRef} className="flex-1 overflow-auto">
        <div
          style={{
            height: `${rowVirtualizer.getTotalSize()}px`,
            width: '100%',
            position: 'relative',
          }}
        >
          {rowVirtualizer.getVirtualItems().map(virtualRow => {
            const row = filteredRows[virtualRow.index];
            return (
              <DiffRowComponent
                key={row.rowKey}
                row={row}
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  width: '100%',
                  height: `${virtualRow.size}px`,
                  transform: `translateY(${virtualRow.start}px)`,
                }}
              />
            );
          })}
        </div>
      </div>
    </div>
  );
}

interface DiffRowComponentProps {
  row: DiffRow;
  style: React.CSSProperties;
}

function DiffRowComponent({ row, style }: DiffRowComponentProps) {
  const statusIcon = useMemo(() => getStatusIcon(row.status), [row.status]);
  const rowClassName = useMemo(() => getRowClassName(row.status), [row.status]);

  return (
    <div
      className={cn('flex items-center border-b border-border transition-colors', rowClassName)}
      style={style}
    >
      {/* Status icon */}
      <div className="w-10 shrink-0 flex items-center justify-center border-r border-border">
        {statusIcon}
      </div>

      {/* Cells */}
      {row.status === 'removed'
        ? row.leftCells.map((cell, colIdx) => (
            <DiffCell
              key={colIdx}
              value={formatDiffValue(cell.value)}
              changed={cell.changed}
              status={row.status}
            />
          ))
        : row.status === 'added'
          ? row.rightCells.map((cell, colIdx) => (
              <DiffCell
                key={colIdx}
                value={formatDiffValue(cell.value)}
                changed={cell.changed}
                status={row.status}
              />
            ))
          : row.status === 'modified'
            ? row.leftCells.map((cell, colIdx) => (
                <DiffCellModified
                  key={colIdx}
                  oldValue={formatDiffValue(cell.value)}
                  newValue={formatDiffValue(row.rightCells[colIdx].value)}
                  changed={cell.changed}
                />
              ))
            : row.leftCells.map((cell, colIdx) => (
                <DiffCell
                  key={colIdx}
                  value={formatDiffValue(cell.value)}
                  changed={false}
                  status={row.status}
                />
              ))}
    </div>
  );
}

interface DiffCellProps {
  value: string;
  changed: boolean;
  status: DiffRowStatus;
}

function DiffCell({ value, changed, status }: DiffCellProps) {
  return (
    <div
      className={cn(
        'flex-1 min-w-[120px] px-3 py-1.5 font-mono text-xs truncate border-r border-border last:border-r-0',
        status === 'unchanged' && 'text-muted-foreground',
        status === 'removed' && changed && 'bg-error/10',
        status === 'added' && changed && 'bg-success/10'
      )}
      title={value}
    >
      {value}
    </div>
  );
}

interface DiffCellModifiedProps {
  oldValue: string;
  newValue: string;
  changed: boolean;
}

function DiffCellModified({ oldValue, newValue, changed }: DiffCellModifiedProps) {
  if (!changed) {
    return (
      <div
        className="flex-1 min-w-[120px] px-3 py-1.5 font-mono text-xs truncate border-r border-border last:border-r-0"
        title={oldValue}
      >
        {oldValue}
      </div>
    );
  }

  return (
    <div
      className="flex-1 min-w-[120px] px-3 py-0.5 font-mono text-xs border-r border-border last:border-r-0 bg-warning/10 overflow-hidden"
      title={`${oldValue} → ${newValue}`}
    >
      <div className="flex flex-col gap-0">
        <span className="line-through text-error/70 truncate text-[10px]">{oldValue}</span>
        <span className="text-success truncate">{newValue}</span>
      </div>
    </div>
  );
}

function getStatusIcon(status: DiffRowStatus) {
  switch (status) {
    case 'added':
      return <PlusCircle size={14} className="text-success" />;
    case 'removed':
      return <MinusCircle size={14} className="text-error" />;
    case 'modified':
      return <ArrowLeftRight size={14} className="text-warning" />;
    case 'unchanged':
      return <CheckCircle2 size={14} className="text-muted-foreground/50" />;
  }
}

function getRowClassName(status: DiffRowStatus) {
  switch (status) {
    case 'added':
      return 'bg-success/5 hover:bg-success/10';
    case 'removed':
      return 'bg-error/5 hover:bg-error/10';
    case 'modified':
      return 'bg-warning/5 hover:bg-warning/10';
    case 'unchanged':
      return 'hover:bg-muted/50';
  }
}
