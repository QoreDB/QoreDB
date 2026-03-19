// SPDX-License-Identifier: Apache-2.0

import {
  ChevronsRight,
  Code,
  Copy,
  FileText,
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
import { Tooltip } from '@/components/ui/tooltip';
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

const borderStateMap: Record<CellExecutionState, string> = {
  idle: 'border-l-transparent',
  running: 'border-l-accent',
  success: 'border-l-green-500',
  error: 'border-l-destructive',
  stale: 'border-l-amber-500',
};

const CellTypeIcon = ({ type }: { type: string }) => {
  switch (type) {
    case 'sql':
    case 'mongo':
      return <Code size={13} className="text-muted-foreground" />;
    case 'markdown':
      return <FileText size={13} className="text-muted-foreground" />;
    default:
      return <Code size={13} className="text-muted-foreground" />;
  }
};

export function NotebookCell({
  cell,
  allCells,
  dialect,
  sessionId,
  connectionDatabase,
  namespace,
  isFocused,
  isFirst: _isFirst,
  isLast: _isLast,
  onFocus,
  onSourceChange,
  onExecute,
  onDelete,
  onMoveUp: _onMoveUp,
  onMoveDown: _onMoveDown,
  onCancel,
  onDuplicate,
  onConvertType,
  onToggleCollapsed,
  onRunFromHere,
}: NotebookCellProps) {
  const { t } = useTranslation();
  const [showSuccessBorder, setShowSuccessBorder] = useState(false);

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

  const contextMenuItems = (
    <>
      {isExecutable && (
        <ContextMenuItem onClick={onExecute}>
          <Play size={14} className="mr-2" />
          {t('notebook.executeCell')}
        </ContextMenuItem>
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
            'group relative rounded-lg border border-border/50 bg-card transition-all',
            'border-l-[3px]',
            borderStateMap[borderState],
            isFocused ? 'ring-1 ring-ring/40 border-border shadow-sm' : 'hover:border-border/80'
          )}
          onClick={onFocus}
        >
          {/* Drag handle - left edge, on hover */}
          <div className="absolute -left-0.5 top-0 bottom-0 flex items-center opacity-0 group-hover:opacity-60 transition-opacity z-10">
            <div className="cursor-grab active:cursor-grabbing p-0.5 text-muted-foreground">
              <GripVertical size={12} />
            </div>
          </div>

          {/* Floating action bar - top right, on hover */}
          <div className="absolute top-1.5 right-1.5 flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity z-10 bg-card/95 rounded-md border border-border/60 shadow-sm px-0.5 py-0.5">
            {isExecutable && !isRunning && (
              <Tooltip content={t('notebook.executeCell')} side="bottom">
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6"
                  onClick={e => {
                    e.stopPropagation();
                    onExecute();
                  }}
                >
                  <Play size={12} />
                </Button>
              </Tooltip>
            )}
            {isRunning && (
              <Tooltip content={t('notebook.cancelExecution')} side="bottom">
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6 text-destructive"
                  onClick={e => {
                    e.stopPropagation();
                    onCancel?.();
                  }}
                >
                  <Square size={12} />
                </Button>
              </Tooltip>
            )}

            {/* More actions dropdown */}
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6"
                  onClick={e => e.stopPropagation()}
                  title={t('toolbar.moreActions')}
                >
                  <MoreHorizontal size={12} />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" side="bottom" className="w-44">
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
          </div>

          {/* Running indicator - inline spinner */}
          {isRunning && (
            <div className="absolute top-2 right-2 z-20">
              <Loader2 size={14} className="animate-spin text-accent" />
            </div>
          )}

          {/* Cell type indicator + content */}
          <div className="px-4 pt-2 pb-1">
            {/* Cell type header */}
            <div className="flex items-center gap-1.5 mb-1.5">
              <CellTypeIcon type={cell.type} />
              <span className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">
                {cell.type}
              </span>
              {cell.executionState === 'stale' && (
                <span className="text-[10px] text-amber-500 font-medium ml-1">
                  {t('notebook.stale')}
                </span>
              )}
            </div>

            {/* Cell content */}
            {isCollapsed ? (
              <div className="text-xs text-muted-foreground italic truncate py-1">
                {cell.source.split('\n')[0] || t('notebook.cellEmpty')}
              </div>
            ) : (
              <>
                {(cell.type === 'sql' || cell.type === 'mongo') && (
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
                {cell.type === 'chart' && allCells && <ChartCell cell={cell} allCells={allCells} />}
              </>
            )}
          </div>

          {/* Execution metadata footer */}
          {cell.executionTimeMs !== undefined &&
            cell.executionCount !== undefined &&
            cell.executionCount > 0 && (
              <div className="flex items-center gap-1.5 px-4 pb-2 text-[11px] text-muted-foreground">
                <span className="font-mono">#{cell.executionCount}</span>
                <span className="text-border">·</span>
                <span>{cell.executionTimeMs}ms</span>
                {cell.lastResult?.totalRows !== undefined && (
                  <>
                    <span className="text-border">·</span>
                    <span>{t('notebook.rowCount', { count: cell.lastResult.totalRows })}</span>
                  </>
                )}
              </div>
            )}
        </div>
      </ContextMenuTrigger>

      <ContextMenuContent>{contextMenuItems}</ContextMenuContent>
    </ContextMenu>
  );
}
