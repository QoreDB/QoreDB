// SPDX-License-Identifier: Apache-2.0

import { useVirtualizer } from '@tanstack/react-virtual';
import { Reorder } from 'framer-motion';
import { Code, FileText } from 'lucide-react';
import { useCallback, useRef } from 'react';
import { Button } from '@/components/ui/button';
import type { Driver } from '@/lib/drivers';
import type { CellType, NotebookCell as NotebookCellType } from '@/lib/notebookTypes';
import type { Namespace } from '@/lib/tauri';
import { NotebookCell } from './cells/NotebookCell';

const VIRTUALIZATION_THRESHOLD = 30;

interface NotebookCellListProps {
  cells: NotebookCellType[];
  focusedCellId: string | null;
  dialect?: Driver;
  sessionId?: string | null;
  connectionDatabase?: string;
  namespace?: Namespace | null;
  onReorderCells: (newOrder: NotebookCellType[]) => void;
  onFocusCell: (cellId: string) => void;
  onSourceChange: (cellId: string, source: string) => void;
  onExecuteCell: (cellId: string) => void;
  onDeleteCell: (cellId: string) => void;
  onMoveCellUp: (cellId: string) => void;
  onMoveCellDown: (cellId: string) => void;
  onAddCell: (type: CellType, afterCellId?: string) => void;
  onCancelExecution?: () => void;
  onDuplicateCell?: (cellId: string) => void;
  onConvertCellType?: (cellId: string) => void;
  onToggleCellCollapsed?: (cellId: string) => void;
  onExecuteFromHere?: (cellId: string) => void;
}

function AddCellDivider({
  afterCellId,
  onAddCell,
}: {
  afterCellId?: string;
  onAddCell: (type: CellType, afterCellId?: string) => void;
}) {
  return (
    <div className="flex items-center justify-center h-6 group/divider relative">
      <div className="w-full h-px bg-border group-hover/divider:bg-accent/50 transition-colors" />
      <div className="absolute flex items-center gap-1 opacity-0 group-hover/divider:opacity-100 transition-opacity">
        <Button
          variant="outline"
          size="icon"
          className="h-5 w-5 rounded-full bg-background"
          onClick={() => onAddCell('sql', afterCellId)}
        >
          <Code size={10} />
        </Button>
        <Button
          variant="outline"
          size="icon"
          className="h-5 w-5 rounded-full bg-background"
          onClick={() => onAddCell('markdown', afterCellId)}
        >
          <FileText size={10} />
        </Button>
      </div>
    </div>
  );
}

interface CellRendererProps {
  cell: NotebookCellType;
  allCells: NotebookCellType[];
  index: number;
  total: number;
  props: Omit<NotebookCellListProps, 'cells' | 'onReorderCells'>;
}

function CellRenderer({ cell, allCells, index, total, props }: CellRendererProps) {
  return (
    <>
      <NotebookCell
        cell={cell}
        allCells={allCells}
        dialect={props.dialect}
        sessionId={props.sessionId}
        connectionDatabase={props.connectionDatabase}
        namespace={props.namespace}
        isFocused={props.focusedCellId === cell.id}
        isFirst={index === 0}
        isLast={index === total - 1}
        onFocus={() => props.onFocusCell(cell.id)}
        onSourceChange={source => props.onSourceChange(cell.id, source)}
        onExecute={() => props.onExecuteCell(cell.id)}
        onDelete={() => props.onDeleteCell(cell.id)}
        onMoveUp={() => props.onMoveCellUp(cell.id)}
        onMoveDown={() => props.onMoveCellDown(cell.id)}
        onCancel={props.onCancelExecution}
        onDuplicate={props.onDuplicateCell && (() => props.onDuplicateCell?.(cell.id))}
        onConvertType={props.onConvertCellType && (() => props.onConvertCellType?.(cell.id))}
        onToggleCollapsed={
          props.onToggleCellCollapsed && (() => props.onToggleCellCollapsed?.(cell.id))
        }
        onRunFromHere={props.onExecuteFromHere && (() => props.onExecuteFromHere?.(cell.id))}
      />
      <AddCellDivider afterCellId={cell.id} onAddCell={props.onAddCell} />
    </>
  );
}

export function NotebookCellList({ cells, onReorderCells, ...rest }: NotebookCellListProps) {
  const useVirtual = cells.length >= VIRTUALIZATION_THRESHOLD;

  if (useVirtual) {
    return <VirtualizedCellList cells={cells} {...rest} />;
  }

  return <ReorderCellList cells={cells} onReorderCells={onReorderCells} {...rest} />;
}

function ReorderCellList({ cells, onReorderCells, ...rest }: NotebookCellListProps) {
  const handleReorder = useCallback(
    (newOrder: NotebookCellType[]) => {
      onReorderCells(newOrder);
    },
    [onReorderCells]
  );

  return (
    <div className="flex-1 overflow-y-auto px-6 py-4">
      <Reorder.Group
        axis="y"
        values={cells}
        onReorder={handleReorder}
        className="flex flex-col gap-2"
      >
        {cells.map((cell, index) => (
          <Reorder.Item
            key={cell.id}
            value={cell}
            layout="position"
            transition={{ duration: 0.15 }}
          >
            <CellRenderer
              cell={cell}
              allCells={cells}
              index={index}
              total={cells.length}
              props={rest}
            />
          </Reorder.Item>
        ))}
      </Reorder.Group>
    </div>
  );
}

function VirtualizedCellList({ cells, ...rest }: Omit<NotebookCellListProps, 'onReorderCells'>) {
  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: cells.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 120,
    overscan: 5,
  });

  return (
    <div ref={parentRef} className="flex-1 overflow-y-auto px-6 py-4">
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: '100%',
          position: 'relative',
        }}
      >
        {virtualizer.getVirtualItems().map(virtualItem => {
          const cell = cells[virtualItem.index];
          return (
            <div
              key={cell.id}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                transform: `translateY(${virtualItem.start}px)`,
              }}
              ref={virtualizer.measureElement}
              data-index={virtualItem.index}
            >
              <CellRenderer
                cell={cell}
                allCells={cells}
                index={virtualItem.index}
                total={cells.length}
                props={rest}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}
