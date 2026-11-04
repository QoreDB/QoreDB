// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useState } from 'react';
import { useNotebook } from '@/hooks/useNotebook';
import { useSchemaCache } from '@/hooks/useSchemaCache';
import { useTourManager } from '@/hooks/useTourManager';
import { aiExplainResult, aiSummarizeSchema } from '@/lib/ai';
import { Driver } from '@/lib/connection/drivers';
import { notify } from '@/lib/notify';
import type { DriverCapabilities, Environment, Namespace } from '@/lib/tauri';
import { useAiPreferences } from '@/providers/AiPreferencesProvider';
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
  const tourManager = useTourManager();

  useEffect(() => {
    if (sessionId && tourManager.shouldShowTour('first-notebook')) {
      const timer = setTimeout(() => tourManager.startTour('first-notebook'), 800);
      return () => clearTimeout(timer);
    }
  }, [sessionId]);

  const schemaCache = useSchemaCache(sessionId || '');
  const [namespaces, setNamespaces] = useState<Namespace[]>([]);
  const [selectedNamespace, setSelectedNamespace] = useState<Namespace | null>(
    activeNamespace ?? null
  );

  useEffect(() => {
    if (!sessionId) return;
    schemaCache.getNamespaces().then(ns => {
      setNamespaces(ns);
      if (!selectedNamespace && ns.length > 0) {
        const defaultSchema = [Driver.SqlServer, Driver.AzureSql, Driver.Synapse].includes(dialect)
          ? 'dbo'
          : 'public';
        const match = connectionDatabase
          ? (ns.find(n => n.database === connectionDatabase && n.schema === defaultSchema) ??
            ns.find(n => n.database === connectionDatabase))
          : ns[0];
        setSelectedNamespace(match ?? ns[0]);
      }
    });
  }, [sessionId]);

  const effectiveNamespace = selectedNamespace ?? activeNamespace ?? null;

  const nb = useNotebook({
    tabId,
    sessionId,
    dialect,
    namespace: effectiveNamespace,
    connectionDatabase,
    environment,
    readOnly,
    initialPath,
    initialQuery,
    onDirtyChange,
  });

  const hasVariables = Object.keys(nb.notebook.variables).length > 0;
  const [showVariableBar, setShowVariableBar] = useState(hasVariables);
  const { getConfig, isReady: aiReady } = useAiPreferences();
  const [generatingDocumentation, setGeneratingDocumentation] = useState(false);

  const handleGenerateDocumentation = useCallback(async () => {
    if (!sessionId || !aiReady || generatingDocumentation) return;
    setGeneratingDocumentation(true);
    try {
      const sqlCells = nb.notebook.cells.filter(
        c => (c.type === 'sql' || c.type === 'mongo') && c.source.trim().length > 0
      );
      if (sqlCells.length > 0) {
        const combinedQueries = sqlCells
          .map((c, idx) => `-- Cell ${idx + 1}\n${c.source.trim()}`)
          .join('\n\n');
        const response = await aiExplainResult(
          sessionId,
          combinedQueries,
          `Notebook "${nb.notebook.metadata.title}" with ${sqlCells.length} query cell(s). Generate structured Markdown documentation explaining the purpose and steps of this notebook.`,
          getConfig(),
          effectiveNamespace ?? undefined
        );
        nb.addCell('markdown', nb.focusedCellId ?? undefined, response.content);
      } else {
        const response = await aiSummarizeSchema(
          sessionId,
          getConfig(),
          effectiveNamespace ?? undefined
        );
        nb.addCell('markdown', nb.focusedCellId ?? undefined, response.content);
      }
    } catch (err) {
      notify.error(err instanceof Error ? err.message : String(err));
    } finally {
      setGeneratingDocumentation(false);
    }
  }, [sessionId, aiReady, generatingDocumentation, nb, getConfig, effectiveNamespace]);

  useEffect(() => {
    if (hasVariables) setShowVariableBar(true);
  }, [hasVariables]);

  const handleToggleVariables = useCallback(() => {
    if (showVariableBar && !hasVariables) {
      setShowVariableBar(false);
    } else if (showVariableBar) {
      setShowVariableBar(false);
    } else {
      setShowVariableBar(true);
    }
  }, [showVariableBar, hasVariables]);

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
    <div className="flex flex-col h-full min-h-0">
      <NotebookToolbar
        title={nb.notebook.metadata.title}
        isDirty={nb.isDirty}
        isExecuting={nb.isExecuting}
        canUndo={nb.canUndo}
        canRedo={nb.canRedo}
        hasVariables={hasVariables}
        cellCount={nb.notebook.cells.length}
        namespaces={namespaces}
        selectedNamespace={effectiveNamespace}
        onNamespaceChange={setSelectedNamespace}
        onTitleChange={nb.setTitle}
        onSave={nb.save}
        onSaveAs={nb.saveAs}
        onOpen={nb.openFromFile}
        onAddCell={type => nb.addCell(type, nb.focusedCellId ?? undefined)}
        onExecuteAll={() => nb.executeAll()}
        onClearAll={nb.clearAllResults}
        onCancel={nb.cancelExecution}
        onUndo={nb.undo}
        onRedo={nb.redo}
        onImport={nb.importFromFile}
        onExport={format => nb.exportToFile(format)}
        onToggleVariables={handleToggleVariables}
        onGenerateDocumentation={aiReady && sessionId ? handleGenerateDocumentation : undefined}
        generatingDocumentation={generatingDocumentation}
      />
      {showVariableBar && (
        <NotebookVariableBar
          variables={nb.notebook.variables}
          onUpdateVariable={nb.updateVariable}
          onAddVariable={nb.addVariable}
          onRemoveVariable={nb.removeVariable}
        />
      )}
      <NotebookCellList
        data-tour="notebook-cells"
        cells={nb.notebook.cells}
        focusedCellId={nb.focusedCellId}
        dialect={dialect}
        sessionId={sessionId}
        connectionDatabase={connectionDatabase}
        namespace={effectiveNamespace}
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
