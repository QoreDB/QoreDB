// SPDX-License-Identifier: Apache-2.0

import { ChevronDown, ChevronUp, GripVertical, Loader2, Play, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Tooltip } from '@/components/ui/tooltip';
import type { Driver } from '@/lib/drivers';
import type { CellExecutionState, NotebookCell as NotebookCellType } from '@/lib/notebookTypes';
import type { Namespace } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { MarkdownCell } from './MarkdownCell';
import { SqlCell } from './SqlCell';

interface NotebookCellProps {
  cell: NotebookCellType;
  dialect?: Driver;
  sessionId?: string | null;
  connectionDatabase?: string;
  namespace?: Namespace | null;
  isFocused: boolean;
  isFirst: boolean;
  isLast: boolean;
  onFocus: () => void;
  onSourceChange: (source: string) => void;
  onExecute: () => void;
  onDelete: () => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
}

const borderColorMap: Record<CellExecutionState, string> = {
  idle: 'border-l-border',
  running: 'border-l-accent',
  success: 'border-l-green-500',
  error: 'border-l-destructive',
};

export function NotebookCell({
  cell,
  dialect,
  sessionId,
  connectionDatabase,
  namespace,
  isFocused,
  isFirst,
  isLast,
  onFocus,
  onSourceChange,
  onExecute,
  onDelete,
  onMoveUp,
  onMoveDown,
}: NotebookCellProps) {
  const { t } = useTranslation();
  const [showSuccessBorder, setShowSuccessBorder] = useState(false);

  // Flash success border for 2s, then revert to idle
  useEffect(() => {
    if (cell.executionState === 'success') {
      setShowSuccessBorder(true);
      const timer = setTimeout(() => setShowSuccessBorder(false), 2000);
      return () => clearTimeout(timer);
    }
    setShowSuccessBorder(false);
  }, [cell.executionState, cell.executionCount]);

  const borderState: CellExecutionState =
    cell.executionState === 'success' && !showSuccessBorder
      ? 'idle'
      : (cell.executionState ?? 'idle');

  const isExecutable = cell.type === 'sql' || cell.type === 'mongo';

  const handleDelete = useCallback(() => {
    if (cell.source.trim()) {
      if (!window.confirm(t('notebook.deleteCellConfirm'))) return;
    }
    onDelete();
  }, [cell.source, onDelete, t]);

  return (
    <div
      className={cn(
        'group relative border-l-2 rounded-md bg-background transition-colors',
        borderColorMap[borderState],
        isFocused && 'ring-1 ring-ring/50'
      )}
      onClick={onFocus}
    >
      {/* Drag handle */}
      <div className="absolute -left-1 top-0 bottom-0 flex items-center opacity-0 group-hover:opacity-100 transition-opacity">
        <div className="cursor-grab active:cursor-grabbing p-0.5 text-muted-foreground hover:text-foreground">
          <GripVertical size={14} />
        </div>
      </div>

      <div className="flex items-start gap-1 pl-5 pr-2 py-1">
        {/* Cell type badge */}
        <div className="shrink-0 mt-1.5">
          <span className="text-[10px] font-mono uppercase text-muted-foreground px-1 py-0.5 bg-muted rounded">
            {cell.type}
          </span>
        </div>

        {/* Cell content */}
        <div className="flex-1 min-w-0 py-1">
          {cell.type === 'sql' && (
            <SqlCell
              cell={cell}
              dialect={dialect}
              sessionId={sessionId}
              connectionDatabase={connectionDatabase}
              namespace={namespace}
              onSourceChange={onSourceChange}
              onExecute={onExecute}
            />
          )}
          {cell.type === 'markdown' && <MarkdownCell cell={cell} onSourceChange={onSourceChange} />}
        </div>

        {/* Action buttons */}
        <div className="shrink-0 flex flex-col items-center gap-0.5 pt-1 opacity-0 group-hover:opacity-100 transition-opacity">
          {isExecutable && (
            <Tooltip content={t('notebook.executeCell')} side="left">
              <Button
                variant="ghost"
                size="icon"
                className="h-6 w-6"
                onClick={e => {
                  e.stopPropagation();
                  onExecute();
                }}
                disabled={cell.executionState === 'running'}
              >
                {cell.executionState === 'running' ? (
                  <Loader2 size={12} className="animate-spin" />
                ) : (
                  <Play size={12} />
                )}
              </Button>
            </Tooltip>
          )}
          <Tooltip content={t('notebook.moveCellUp')} side="left">
            <Button
              variant="ghost"
              size="icon"
              className="h-6 w-6"
              onClick={e => {
                e.stopPropagation();
                onMoveUp();
              }}
              disabled={isFirst}
            >
              <ChevronUp size={12} />
            </Button>
          </Tooltip>
          <Tooltip content={t('notebook.moveCellDown')} side="left">
            <Button
              variant="ghost"
              size="icon"
              className="h-6 w-6"
              onClick={e => {
                e.stopPropagation();
                onMoveDown();
              }}
              disabled={isLast}
            >
              <ChevronDown size={12} />
            </Button>
          </Tooltip>
          <Tooltip content={t('notebook.deleteCell')} side="left">
            <Button
              variant="ghost"
              size="icon"
              className="h-6 w-6 text-muted-foreground hover:text-destructive"
              onClick={e => {
                e.stopPropagation();
                handleDelete();
              }}
            >
              <Trash2 size={12} />
            </Button>
          </Tooltip>
        </div>
      </div>

      {/* Execution info */}
      {cell.executionTimeMs !== undefined &&
        cell.executionCount !== undefined &&
        cell.executionCount > 0 && (
          <div className="flex items-center gap-2 px-6 pb-1 text-[10px] text-muted-foreground">
            <span>{t('notebook.executionCount', { count: cell.executionCount })}</span>
            <span>{t('notebook.executionTime', { time: cell.executionTimeMs })}</span>
            {cell.lastResult?.totalRows !== undefined && (
              <span>{t('notebook.rowCount', { count: cell.lastResult.totalRows })}</span>
            )}
          </div>
        )}
    </div>
  );
}
