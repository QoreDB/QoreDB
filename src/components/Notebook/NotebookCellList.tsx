// SPDX-License-Identifier: Apache-2.0

import { Reorder } from 'framer-motion';
import { Code, FileText, Plus } from 'lucide-react';
import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import type { Driver } from '@/lib/drivers';
import type { CellType, NotebookCell as NotebookCellType } from '@/lib/notebookTypes';
import type { Namespace } from '@/lib/tauri';
import { NotebookCell } from './cells/NotebookCell';

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
}

function AddCellDivider({
  afterCellId,
  onAddCell,
}: {
  afterCellId?: string;
  onAddCell: (type: CellType, afterCellId?: string) => void;
}) {
  const { t } = useTranslation();

  return (
    <div className="flex items-center justify-center h-4 group/divider">
      <div className="w-full h-px bg-border group-hover/divider:bg-accent/50 transition-colors" />
      <div className="absolute opacity-0 group-hover/divider:opacity-100 transition-opacity">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="icon" className="h-5 w-5 rounded-full bg-background">
              <Plus size={10} />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="center">
            <DropdownMenuItem onClick={() => onAddCell('sql', afterCellId)}>
              <Code size={14} className="mr-2" />
              {t('notebook.addCellSql')}
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => onAddCell('markdown', afterCellId)}>
              <FileText size={14} className="mr-2" />
              {t('notebook.addCellMarkdown')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
}

export function NotebookCellList({
  cells,
  focusedCellId,
  dialect,
  sessionId,
  connectionDatabase,
  namespace,
  onReorderCells,
  onFocusCell,
  onSourceChange,
  onExecuteCell,
  onDeleteCell,
  onMoveCellUp,
  onMoveCellDown,
  onAddCell,
}: NotebookCellListProps) {
  const handleReorder = useCallback(
    (newOrder: NotebookCellType[]) => {
      onReorderCells(newOrder);
    },
    [onReorderCells]
  );

  return (
    <div className="flex-1 overflow-y-auto px-4 py-3">
      <Reorder.Group
        axis="y"
        values={cells}
        onReorder={handleReorder}
        className="flex flex-col gap-1"
      >
        {cells.map((cell, index) => (
          <Reorder.Item
            key={cell.id}
            value={cell}
            layout="position"
            transition={{ duration: 0.15 }}
          >
            <NotebookCell
              cell={cell}
              dialect={dialect}
              sessionId={sessionId}
              connectionDatabase={connectionDatabase}
              namespace={namespace}
              isFocused={focusedCellId === cell.id}
              isFirst={index === 0}
              isLast={index === cells.length - 1}
              onFocus={() => onFocusCell(cell.id)}
              onSourceChange={source => onSourceChange(cell.id, source)}
              onExecute={() => onExecuteCell(cell.id)}
              onDelete={() => onDeleteCell(cell.id)}
              onMoveUp={() => onMoveCellUp(cell.id)}
              onMoveDown={() => onMoveCellDown(cell.id)}
            />
            <AddCellDivider afterCellId={cell.id} onAddCell={onAddCell} />
          </Reorder.Item>
        ))}
      </Reorder.Group>
    </div>
  );
}
