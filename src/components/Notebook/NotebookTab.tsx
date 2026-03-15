// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect } from 'react';
import { useNotebook } from '@/hooks/useNotebook';
import { Driver } from '@/lib/drivers';
import type { DriverCapabilities, Environment, Namespace } from '@/lib/tauri';
import { NotebookCellList } from './NotebookCellList';
import { NotebookToolbar } from './NotebookToolbar';
import { NotebookVariableBar } from './NotebookVariableBar';

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
  onDirtyChange?: (dirty: boolean) => void;
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
  onDirtyChange,
}: NotebookTabProps) {
  const nb = useNotebook({
    tabId,
    sessionId,
    dialect,
    namespace: activeNamespace,
    connectionDatabase,
    environment,
    readOnly,
    initialPath,
    initialQuery,
    onDirtyChange,
  });

  // Keyboard shortcuts
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;

      // Ctrl+Z: undo
      if (mod && !e.shiftKey && e.key === 'z') {
        e.preventDefault();
        nb.undo();
        return;
      }

      // Ctrl+Shift+Z / Ctrl+Y: redo
      if ((mod && e.shiftKey && e.key === 'z') || (mod && e.key === 'y')) {
        e.preventDefault();
        nb.redo();
        return;
      }

      // Ctrl+S: save
      if (mod && e.key === 's') {
        e.preventDefault();
        nb.save();
        return;
      }

      // Ctrl+Shift+A: execute all
      if (mod && e.shiftKey && e.key === 'a') {
        e.preventDefault();
        nb.executeAll();
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

      // Ctrl+Shift+D: duplicate cell
      if (mod && e.shiftKey && e.key === 'd' && nb.focusedCellId) {
        e.preventDefault();
        nb.duplicateCell(nb.focusedCellId);
        return;
      }

      // Ctrl+Shift+T: convert cell type
      if (mod && e.shiftKey && e.key === 't' && nb.focusedCellId) {
        e.preventDefault();
        nb.convertCellType(nb.focusedCellId);
        return;
      }

      // Ctrl+Up/Down: focus prev/next cell
      if (mod && !e.shiftKey && !e.altKey) {
        if (e.key === 'ArrowUp') {
          e.preventDefault();
          nb.focusPrevCell();
          return;
        }
        if (e.key === 'ArrowDown') {
          e.preventDefault();
          nb.focusNextCell();
          return;
        }
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
        isExecuting={nb.isExecuting}
        onTitleChange={nb.setTitle}
        onSave={nb.save}
        onAddCell={type => nb.addCell(type, nb.focusedCellId ?? undefined)}
        onExecuteAll={() => nb.executeAll()}
        onClearAll={nb.clearAllResults}
        onCancel={nb.cancelExecution}
        onImport={nb.importFromFile}
        onExport={format => nb.exportToFile(format)}
      />
      {Object.keys(nb.notebook.variables).length > 0 && (
        <NotebookVariableBar
          variables={nb.notebook.variables}
          onUpdateVariable={nb.updateVariable}
          onAddVariable={nb.addVariable}
          onRemoveVariable={nb.removeVariable}
        />
      )}
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
        onCancelExecution={nb.cancelExecution}
        onDuplicateCell={nb.duplicateCell}
        onConvertCellType={nb.convertCellType}
        onToggleCellCollapsed={nb.toggleCellCollapsed}
        onExecuteFromHere={cellId => nb.executeFromHere(cellId)}
      />
    </div>
  );
}
