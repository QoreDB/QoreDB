// SPDX-License-Identifier: Apache-2.0

import {
  ChevronDown,
  ChevronsRight,
  ChevronUp,
  Copy,
  FoldVertical,
  GripVertical,
  Loader2,
  MoreHorizontal,
  Play,
  RefreshCw,
  Square,
  Trash2,
  UnfoldVertical,
} from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import type { Driver } from '@/lib/drivers';
import type { CellExecutionState, NotebookCell as NotebookCellType } from '@/lib/notebookTypes';
import type { Namespace } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { ChartCell } from './ChartCell';
import { MarkdownCell } from './MarkdownCell';
import { SqlCell } from './SqlCell';

interface NotebookCellProps {
  cell: NotebookCellType;
  allCells?: NotebookCellType[];
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
  onCancel?: () => void;
  onDuplicate?: () => void;
  onConvertType?: () => void;
  onToggleCollapsed?: () => void;
  onRunFromHere?: () => void;
}

const borderColorMap: Record<CellExecutionState, string> = {
  idle: 'border-l-border',
  running: 'border-l-accent',
  success: 'border-l-green-500',
  error: 'border-l-destructive',
  stale: 'border-l-amber-500 border-dashed',
};

export function NotebookCell({
  cell,
  allCells,
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
  onCancel,
  onDuplicate,
  onConvertType,
  onToggleCollapsed,
  onRunFromHere,
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
  }, [cell.executionState]);

  const borderState: CellExecutionState =
    cell.executionState === 'success' && !showSuccessBorder
      ? 'idle'
      : (cell.executionState ?? 'idle');

  const isExecutable = cell.type === 'sql' || cell.type === 'mongo';
  const isRunning = cell.executionState === 'running';
  const isCollapsed = cell.config?.collapsed;

  const handleDelete = useCallback(() => {
    if (cell.source.trim()) {
      if (!window.confirm(t('notebook.deleteCellConfirm'))) return;
    }
    onDelete();
  }, [cell.source, onDelete, t]);

  // Shared menu items for both context menu and dropdown
  const menuItems = (
    <>
      {isExecutable && (
        <ContextMenuItem onClick={onExecute}>{t('notebook.executeCell')}</ContextMenuItem>
      )}
      {isExecutable && onRunFromHere && (
        <ContextMenuItem onClick={onRunFromHere}>
          <ChevronsRight size={14} className="mr-2" />
          {t('notebook.executeFromHere')}
        </ContextMenuItem>
      )}
      {isExecutable && <ContextMenuSeparator />}
      {onDuplicate && (
        <ContextMenuItem onClick={onDuplicate}>
          <Copy size={14} className="mr-2" />
          {t('notebook.duplicateCell')}
        </ContextMenuItem>
      )}
      {onConvertType && (
        <ContextMenuItem onClick={onConvertType}>
          <RefreshCw size={14} className="mr-2" />
          {t('notebook.convertType')}
        </ContextMenuItem>
      )}
      {onToggleCollapsed && (
        <ContextMenuItem onClick={onToggleCollapsed}>
          {isCollapsed ? (
            <UnfoldVertical size={14} className="mr-2" />
          ) : (
            <FoldVertical size={14} className="mr-2" />
          )}
          {isCollapsed ? t('notebook.expandCell') : t('notebook.collapseCell')}
        </ContextMenuItem>
      )}
      <ContextMenuSeparator />
      <ContextMenuItem onClick={onMoveUp} disabled={isFirst}>
        <ChevronUp size={14} className="mr-2" />
        {t('notebook.moveCellUp')}
      </ContextMenuItem>
      <ContextMenuItem onClick={onMoveDown} disabled={isLast}>
        <ChevronDown size={14} className="mr-2" />
        {t('notebook.moveCellDown')}
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onClick={handleDelete} className="text-destructive">
        <Trash2 size={14} className="mr-2" />
        {t('notebook.deleteCell')}
      </ContextMenuItem>
    </>
  );

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        {/* biome-ignore lint/a11y/useKeyWithClickEvents: focus capture on container */}
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
              {isCollapsed ? (
                <div className="text-xs text-muted-foreground italic truncate px-1">
                  {cell.source.split('\n')[0] || t('notebook.cellEmpty')}
                </div>
              ) : (
                <>
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
                  {cell.type === 'markdown' && (
                    <MarkdownCell cell={cell} onSourceChange={onSourceChange} />
                  )}
                  {cell.type === 'chart' && allCells && (
                    <ChartCell cell={cell} allCells={allCells} />
                  )}
                </>
              )}
            </div>

            {/* Action buttons */}
            <div className="shrink-0 flex flex-col items-center gap-0.5 pt-1 opacity-0 group-hover:opacity-100 transition-opacity">
              {isExecutable && !isRunning && (
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6"
                  title={t('notebook.executeCell')}
                  onClick={e => {
                    e.stopPropagation();
                    onExecute();
                  }}
                >
                  <Play size={12} />
                </Button>
              )}
              {isRunning && (
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6 text-destructive"
                  title={t('notebook.cancelExecution')}
                  onClick={e => {
                    e.stopPropagation();
                    onCancel?.();
                  }}
                >
                  <Square size={12} />
                </Button>
              )}
              {isRunning && <Loader2 size={12} className="animate-spin text-accent" />}
              <Button
                variant="ghost"
                size="icon"
                className="h-6 w-6"
                title={t('notebook.moveCellUp')}
                onClick={e => {
                  e.stopPropagation();
                  onMoveUp();
                }}
                disabled={isFirst}
              >
                <ChevronUp size={12} />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="h-6 w-6"
                title={t('notebook.moveCellDown')}
                onClick={e => {
                  e.stopPropagation();
                  onMoveDown();
                }}
                disabled={isLast}
              >
                <ChevronDown size={12} />
              </Button>

              {/* More actions dropdown */}
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6"
                    onClick={e => e.stopPropagation()}
                  >
                    <MoreHorizontal size={12} />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" side="left">
                  {isExecutable && (
                    <DropdownMenuItem onClick={onExecute}>
                      <Play size={14} className="mr-2" />
                      {t('notebook.executeCell')}
                    </DropdownMenuItem>
                  )}
                  {isExecutable && onRunFromHere && (
                    <DropdownMenuItem onClick={onRunFromHere}>
                      <ChevronsRight size={14} className="mr-2" />
                      {t('notebook.executeFromHere')}
                    </DropdownMenuItem>
                  )}
                  {isExecutable && <DropdownMenuSeparator />}
                  {onDuplicate && (
                    <DropdownMenuItem onClick={onDuplicate}>
                      <Copy size={14} className="mr-2" />
                      {t('notebook.duplicateCell')}
                    </DropdownMenuItem>
                  )}
                  {onConvertType && (
                    <DropdownMenuItem onClick={onConvertType}>
                      <RefreshCw size={14} className="mr-2" />
                      {t('notebook.convertType')}
                    </DropdownMenuItem>
                  )}
                  {onToggleCollapsed && (
                    <DropdownMenuItem onClick={onToggleCollapsed}>
                      {isCollapsed ? (
                        <UnfoldVertical size={14} className="mr-2" />
                      ) : (
                        <FoldVertical size={14} className="mr-2" />
                      )}
                      {isCollapsed ? t('notebook.expandCell') : t('notebook.collapseCell')}
                    </DropdownMenuItem>
                  )}
                  <DropdownMenuSeparator />
                  <DropdownMenuItem onClick={handleDelete} className="text-destructive">
                    <Trash2 size={14} className="mr-2" />
                    {t('notebook.deleteCell')}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>

              <Button
                variant="ghost"
                size="icon"
                className="h-6 w-6 text-muted-foreground hover:text-destructive"
                title={t('notebook.deleteCell')}
                onClick={e => {
                  e.stopPropagation();
                  handleDelete();
                }}
              >
                <Trash2 size={12} />
              </Button>
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
                {cell.executionState === 'stale' && (
                  <span className="text-amber-500 font-medium">{t('notebook.stale')}</span>
                )}
              </div>
            )}
        </div>
      </ContextMenuTrigger>

      <ContextMenuContent>{menuItems}</ContextMenuContent>
    </ContextMenu>
  );
}
