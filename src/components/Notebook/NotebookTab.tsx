// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect } from 'react';
import { Driver } from '@/lib/drivers';
import type { DriverCapabilities, Environment, Namespace } from '@/lib/tauri';
import { useNotebook } from '@/hooks/useNotebook';
import { NotebookCellList } from './NotebookCellList';
import { NotebookToolbar } from './NotebookToolbar';

interface NotebookTabProps {
  tabId: string;
  sessionId: string | null;
  dialect?: Driver;
  driverCapabilities?: DriverCapabilities | null;
  environment?: Environment;
  readOnly?: boolean;
  connectionName?: string;
  connectionDatabase?: string;
  activeNamespace?: Namespace | null;
  initialPath?: string;
  initialQuery?: string;
  onSchemaChange?: () => void;
}

export function NotebookTab({
  tabId,
  sessionId,
  dialect = Driver.Postgres,
  environment = 'development',
  readOnly = false,
  connectionDatabase,
  activeNamespace,
  initialPath,
  initialQuery,
}: NotebookTabProps) {
  const nb = useNotebook({
    tabId,
    sessionId,
    dialect,
    namespace: activeNamespace,
    connectionDatabase,
    environment,
    readOnly,
    initialPath: initialPath,
    initialQuery: initialQuery,
  });

  // Keyboard shortcuts
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;

      // Ctrl+S: save
      if (mod && e.key === 's') {
        e.preventDefault();
        nb.save();
        return;
      }

      // Alt+Up/Down: move cell
      if (e.altKey && nb.focusedCellId) {
        if (e.key === 'ArrowUp') {
          e.preventDefault();
          nb.moveCellUp(nb.focusedCellId);
          return;
        }
        if (e.key === 'ArrowDown') {
          e.preventDefault();
          nb.moveCellDown(nb.focusedCellId);
          return;
        }
      }

      // Ctrl+Shift+Backspace: delete focused cell
      if (mod && e.shiftKey && e.key === 'Backspace' && nb.focusedCellId) {
        e.preventDefault();
        nb.deleteCell(nb.focusedCellId);
        return;
      }

      // Ctrl+Shift+Enter: add SQL cell after focused
      if (mod && e.shiftKey && e.key === 'Enter') {
        e.preventDefault();
        nb.addCell('sql', nb.focusedCellId ?? undefined);
        return;
      }

      // Ctrl+Shift+M: add Markdown cell after focused
      if (mod && e.shiftKey && e.key === 'm') {
        e.preventDefault();
        nb.addCell('markdown', nb.focusedCellId ?? undefined);
        return;
      }
    },
    [nb]
  );

  useEffect(() => {
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  return (
    <div className="flex flex-col h-full">
      <NotebookToolbar
        title={nb.notebook.metadata.title}
        isDirty={nb.isDirty}
        onTitleChange={nb.setTitle}
        onSave={nb.save}
        onAddCell={type => nb.addCell(type, nb.focusedCellId ?? undefined)}
      />
      <NotebookCellList
        cells={nb.notebook.cells}
        focusedCellId={nb.focusedCellId}
        dialect={dialect}
        sessionId={sessionId}
        connectionDatabase={connectionDatabase}
        namespace={activeNamespace}
        onReorderCells={nb.reorderCells}
        onFocusCell={nb.setFocusedCell}
        onSourceChange={nb.updateCellSource}
        onExecuteCell={nb.executeCell}
        onDeleteCell={nb.deleteCell}
        onMoveCellUp={nb.moveCellUp}
        onMoveCellDown={nb.moveCellDown}
        onAddCell={nb.addCell}
      />
    </div>
  );
}
