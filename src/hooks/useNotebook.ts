// SPDX-License-Identifier: Apache-2.0

import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import type { Driver } from '../lib/drivers';
import { exportToHtml, exportToMarkdown } from '../lib/notebookExport';
import { importFromMarkdown, importFromSql } from '../lib/notebookImport';
import { resolveInterCellReferences } from '../lib/notebookInterCellRef';
import {
  clearDraft,
  consumePendingNotebook,
  loadDraft,
  openNotebookFromFile,
  saveDraft,
  saveNotebookToFile,
} from '../lib/notebookIO';
import {
  type CellExecutionState,
  type CellResult,
  type CellType,
  createCell,
  createEmptyNotebook,
  type NotebookCell,
  type NotebookVariable,
  type QoreNotebook,
} from '../lib/notebookTypes';
import { extractVariableReferences, substituteVariables } from '../lib/notebookVariables';
import { cancelQuery, executeQuery, type Namespace } from '../lib/tauri';
import { useNotebookHistory } from './useNotebookHistory';

export interface UseNotebookOptions {
  tabId: string;
  sessionId: string | null;
  dialect?: Driver;
  namespace?: Namespace | null;
  connectionDatabase?: string;
  environment?: string;
  readOnly?: boolean;
  initialNotebook?: QoreNotebook;
  initialPath?: string;
  initialQuery?: string;
  onDirtyChange?: (dirty: boolean) => void;
}

export interface UseNotebookReturn {
  notebook: QoreNotebook;
  path: string | null;
  isDirty: boolean;
  focusedCellId: string | null;
  isExecuting: boolean;
  executingCellId: string | null;
  // Cell operations
  addCell: (type: CellType, afterCellId?: string) => void;
  deleteCell: (cellId: string) => void;
  moveCellUp: (cellId: string) => void;
  moveCellDown: (cellId: string) => void;
  reorderCells: (newOrder: NotebookCell[]) => void;
  updateCellSource: (cellId: string, source: string) => void;
  setFocusedCell: (cellId: string | null) => void;
  // Execution
  executeCell: (cellId: string) => Promise<void>;
  executeAll: (continueOnError?: boolean) => Promise<void>;
  executeFromHere: (cellId: string, continueOnError?: boolean) => Promise<void>;
  cancelExecution: () => void;
  clearAllResults: () => void;
  // Cell advanced operations
  duplicateCell: (cellId: string) => void;
  convertCellType: (cellId: string) => void;
  toggleCellCollapsed: (cellId: string) => void;
  focusPrevCell: () => void;
  focusNextCell: () => void;
  // Variables
  updateVariable: (name: string, value: string) => void;
  addVariable: (variable: NotebookVariable) => void;
  removeVariable: (name: string) => void;
  // Undo/Redo
  undo: () => void;
  redo: () => void;
  canUndo: boolean;
  canRedo: boolean;
  // File operations
  save: () => Promise<void>;
  saveAs: () => Promise<void>;
  openFromFile: () => Promise<void>;
  importFromFile: () => Promise<void>;
  exportToFile: (format: 'markdown' | 'html', includeResults?: boolean) => Promise<void>;
  // Metadata
  setTitle: (title: string) => void;
}

export function useNotebook(options: UseNotebookOptions): UseNotebookReturn {
  const { tabId, sessionId, namespace, initialPath, initialQuery, onDirtyChange } = options;
  const { t } = useTranslation();

  const [notebook, setNotebook] = useState<QoreNotebook>(() => {
    // Priority: provided notebook > pending (opened from file menu) > draft > new with initialQuery > empty
    if (options.initialNotebook) return options.initialNotebook;
    if (initialPath) {
      const pending = consumePendingNotebook(initialPath);
      if (pending) return pending;
    }
    const draft = loadDraft(tabId);
    if (draft) return draft;
    if (initialQuery) {
      const nb = createEmptyNotebook();
      nb.cells[0].source = initialQuery;
      return nb;
    }
    return createEmptyNotebook();
  });

  const [path, setPath] = useState<string | null>(initialPath ?? null);
  const [isDirty, setIsDirty] = useState(false);
  const [focusedCellId, setFocusedCell] = useState<string | null>(notebook.cells[0]?.id ?? null);
  const [executingCellId, setExecutingCellId] = useState<string | null>(null);

  const notebookRef = useRef(notebook);
  notebookRef.current = notebook;

  const onDirtyChangeRef = useRef(onDirtyChange);
  onDirtyChangeRef.current = onDirtyChange;

  const history = useNotebookHistory();

  // Abort controller for batch execution (Run All / Run From Here)
  const abortRef = useRef<AbortController | null>(null);
  // Track the current queryId for cancellation
  const activeQueryIdRef = useRef<string | null>(null);

  // --- Sync dirty state ---
  useEffect(() => {
    onDirtyChangeRef.current?.(isDirty);
  }, [isDirty]);

  // --- Auto-save draft every 30s ---
  useEffect(() => {
    const interval = setInterval(() => {
      if (isDirty) {
        saveDraft(tabId, notebookRef.current);
      }
    }, 30_000);
    return () => clearInterval(interval);
  }, [tabId, isDirty]);

  // --- Helpers ---

  const updateNotebook = useCallback(
    (updater: (prev: QoreNotebook) => QoreNotebook) => {
      // Push current state for undo BEFORE updating (outside setNotebook to avoid nested setState)
      history.pushState(notebookRef.current);
      setNotebook(prev => {
        const next = updater(prev);
        return next;
      });
      setIsDirty(true);
    },
    [history]
  );

  const undo = useCallback(() => {
    const restored = history.undo(notebookRef.current);
    if (restored) {
      setNotebook(restored);
      setIsDirty(true);
    }
  }, [history]);

  const redo = useCallback(() => {
    const restored = history.redo(notebookRef.current);
    if (restored) {
      setNotebook(restored);
      setIsDirty(true);
    }
  }, [history]);

  const updateCell = useCallback(
    (cellId: string, updater: (cell: NotebookCell) => NotebookCell) => {
      updateNotebook(nb => ({
        ...nb,
        cells: nb.cells.map(c => (c.id === cellId ? updater(c) : c)),
      }));
    },
    [updateNotebook]
  );

  // --- Cell CRUD ---

  const addCell = useCallback(
    (type: CellType, afterCellId?: string) => {
      const cell = createCell(type);
      updateNotebook(nb => {
        const cells = [...nb.cells];
        if (afterCellId) {
          const idx = cells.findIndex(c => c.id === afterCellId);
          cells.splice(idx + 1, 0, cell);
        } else {
          cells.push(cell);
        }
        return { ...nb, cells };
      });
      setFocusedCell(cell.id);
    },
    [updateNotebook]
  );

  const deleteCell = useCallback(
    (cellId: string) => {
      updateNotebook(nb => {
        if (nb.cells.length <= 1) return nb;
        const cells = nb.cells.filter(c => c.id !== cellId);
        return { ...nb, cells };
      });
      setFocusedCell(prev => {
        if (prev === cellId) {
          const cells = notebookRef.current.cells;
          const idx = cells.findIndex(c => c.id === cellId);
          const next = cells[idx + 1] ?? cells[idx - 1];
          return next?.id ?? null;
        }
        return prev;
      });
    },
    [updateNotebook]
  );

  const moveCellUp = useCallback(
    (cellId: string) => {
      updateNotebook(nb => {
        const cells = [...nb.cells];
        const idx = cells.findIndex(c => c.id === cellId);
        if (idx <= 0) return nb;
        [cells[idx - 1], cells[idx]] = [cells[idx], cells[idx - 1]];
        return { ...nb, cells };
      });
    },
    [updateNotebook]
  );

  const moveCellDown = useCallback(
    (cellId: string) => {
      updateNotebook(nb => {
        const cells = [...nb.cells];
        const idx = cells.findIndex(c => c.id === cellId);
        if (idx < 0 || idx >= cells.length - 1) return nb;
        [cells[idx], cells[idx + 1]] = [cells[idx + 1], cells[idx]];
        return { ...nb, cells };
      });
    },
    [updateNotebook]
  );

  const reorderCells = useCallback(
    (newOrder: NotebookCell[]) => {
      updateNotebook(nb => ({ ...nb, cells: newOrder }));
    },
    [updateNotebook]
  );

  const updateCellSource = useCallback(
    (cellId: string, source: string) => {
      updateCell(cellId, cell => {
        // Mark as stale if was previously executed
        const wasExecuted = cell.executionState === 'success' || cell.executionState === 'error';
        return {
          ...cell,
          source,
          executionState: wasExecuted ? ('stale' as CellExecutionState) : cell.executionState,
        };
      });
    },
    [updateCell]
  );

  // --- Execution ---

  const executeSingleCell = useCallback(
    async (cellId: string, signal?: AbortSignal) => {
      if (!sessionId) {
        toast.error(t('query.noConnectionError'));
        return;
      }

      const cell = notebookRef.current.cells.find(c => c.id === cellId);
      if (!cell || (cell.type !== 'sql' && cell.type !== 'mongo')) return;
      if (!cell.source.trim()) return;

      if (signal?.aborted) return;

      const queryId = crypto.randomUUID();
      activeQueryIdRef.current = queryId;
      setExecutingCellId(cellId);
      setFocusedCell(cellId);
      updateCell(cellId, c => ({
        ...c,
        executionState: 'running' as CellExecutionState,
      }));

      const startTime = performance.now();

      try {
        const cellNamespace = cell.config?.namespace ?? namespace ?? undefined;
        let resolvedSource = substituteVariables(cell.source, notebookRef.current.variables);
        resolvedSource = resolveInterCellReferences(resolvedSource, notebookRef.current.cells);
        const response = await executeQuery(sessionId, resolvedSource, {
          namespace: cellNamespace,
          queryId,
        });

        if (signal?.aborted) return;

        const totalTimeMs = Math.round(performance.now() - startTime);

        if (response.success && response.result) {
          const result: CellResult = {
            type: 'table',
            columns: response.result.columns,
            rows: response.result.rows,
            totalRows: response.result.rows.length,
            affectedRows: response.result.affected_rows,
          };

          // If it's a mutation with no rows, show a message instead
          if (
            response.result.columns.length === 0 &&
            response.result.rows.length === 0 &&
            response.result.affected_rows !== undefined
          ) {
            result.type = 'message';
            result.message = t('results.affectedRows', {
              count: response.result.affected_rows,
            });
          }

          updateCell(cellId, c => ({
            ...c,
            lastResult: result,
            executionState: 'success',
            executionCount: (c.executionCount ?? 0) + 1,
            executedAt: new Date().toISOString(),
            executionTimeMs: totalTimeMs,
          }));
        } else {
          updateCell(cellId, c => ({
            ...c,
            lastResult: {
              type: 'error',
              error: response.error ?? t('query.unknownError'),
            },
            executionState: 'error',
            executionCount: (c.executionCount ?? 0) + 1,
            executedAt: new Date().toISOString(),
            executionTimeMs: totalTimeMs,
          }));
          throw new Error(response.error ?? t('query.unknownError'));
        }
      } catch (err) {
        if (signal?.aborted) return;

        const errorMessage = err instanceof Error ? err.message : String(err);
        // Only update cell if it wasn't already updated (i.e. it was a caught exception, not a re-throw from above)
        const currentCell = notebookRef.current.cells.find(c => c.id === cellId);
        if (currentCell?.executionState === 'running') {
          updateCell(cellId, c => ({
            ...c,
            lastResult: { type: 'error', error: errorMessage },
            executionState: 'error',
            executionCount: (c.executionCount ?? 0) + 1,
            executedAt: new Date().toISOString(),
            executionTimeMs: Math.round(performance.now() - startTime),
          }));
        }
        throw err;
      } finally {
        activeQueryIdRef.current = null;
        setExecutingCellId(null);
      }
    },
    [sessionId, namespace, updateCell, t]
  );

  const executeCell = useCallback(
    async (cellId: string) => {
      try {
        await executeSingleCell(cellId);
      } catch {
        // Error already handled in executeSingleCell via updateCell
      }
    },
    [executeSingleCell]
  );

  const executeBatch = useCallback(
    async (cellIds: string[], continueOnError: boolean) => {
      if (!sessionId) {
        toast.error(t('query.noConnectionError'));
        return;
      }

      const abort = new AbortController();
      abortRef.current = abort;

      try {
        for (const cellId of cellIds) {
          if (abort.signal.aborted) break;

          const cell = notebookRef.current.cells.find(c => c.id === cellId);
          if (!cell || (cell.type !== 'sql' && cell.type !== 'mongo')) continue;
          if (!cell.source.trim()) continue;

          try {
            await executeSingleCell(cellId, abort.signal);
          } catch {
            if (!continueOnError) break;
          }
        }

        if (!abort.signal.aborted) {
          toast.success(t('notebook.allCellsExecuted'));
        }
      } finally {
        abortRef.current = null;
      }
    },
    [sessionId, executeSingleCell, t]
  );

  const executeAll = useCallback(
    async (continueOnError = false) => {
      const cellIds = notebookRef.current.cells.map(c => c.id);
      await executeBatch(cellIds, continueOnError);
    },
    [executeBatch]
  );

  const executeFromHere = useCallback(
    async (cellId: string, continueOnError = false) => {
      const cells = notebookRef.current.cells;
      const startIdx = cells.findIndex(c => c.id === cellId);
      if (startIdx < 0) return;
      const cellIds = cells.slice(startIdx).map(c => c.id);
      await executeBatch(cellIds, continueOnError);
    },
    [executeBatch]
  );

  const cancelExecution = useCallback(() => {
    // Signal the batch loop to stop
    abortRef.current?.abort();
    abortRef.current = null;

    // Cancel the in-flight query
    if (sessionId && activeQueryIdRef.current) {
      cancelQuery(sessionId, activeQueryIdRef.current).catch(() => {});
    }

    // Reset currently executing cell to error/cancelled state
    const currentCellId = executingCellId;
    if (currentCellId) {
      updateCell(currentCellId, c => ({
        ...c,
        executionState: 'error',
        lastResult: { type: 'error', error: t('notebook.executionStopped') },
      }));
      setExecutingCellId(null);
    }

    toast.info(t('notebook.executionStopped'));
  }, [sessionId, executingCellId, updateCell, t]);

  const clearAllResults = useCallback(() => {
    updateNotebook(nb => ({
      ...nb,
      cells: nb.cells.map(cell => ({
        ...cell,
        lastResult: undefined,
        executionState: 'idle' as CellExecutionState,
        executionCount: 0,
        executedAt: undefined,
        executionTimeMs: undefined,
      })),
    }));
  }, [updateNotebook]);

  // --- Cell advanced operations ---

  const duplicateCell = useCallback(
    (cellId: string) => {
      updateNotebook(nb => {
        const idx = nb.cells.findIndex(c => c.id === cellId);
        if (idx < 0) return nb;
        const original = nb.cells[idx];
        const clone = createCell(original.type, original.source);
        clone.config = original.config ? { ...original.config } : undefined;
        const cells = [...nb.cells];
        cells.splice(idx + 1, 0, clone);
        return { ...nb, cells };
      });
    },
    [updateNotebook]
  );

  const convertCellType = useCallback(
    (cellId: string) => {
      updateCell(cellId, cell => {
        const newType = cell.type === 'sql' ? 'markdown' : 'sql';
        return {
          ...cell,
          type: newType as CellType,
          lastResult: undefined,
          executionState: 'idle' as CellExecutionState,
          executionCount: 0,
          executedAt: undefined,
          executionTimeMs: undefined,
        };
      });
    },
    [updateCell]
  );

  const toggleCellCollapsed = useCallback(
    (cellId: string) => {
      updateCell(cellId, cell => ({
        ...cell,
        config: {
          ...cell.config,
          collapsed: !cell.config?.collapsed,
        },
      }));
    },
    [updateCell]
  );

  const focusPrevCell = useCallback(() => {
    const cells = notebookRef.current.cells;
    const idx = cells.findIndex(c => c.id === focusedCellId);
    if (idx > 0) setFocusedCell(cells[idx - 1].id);
  }, [focusedCellId]);

  const focusNextCell = useCallback(() => {
    const cells = notebookRef.current.cells;
    const idx = cells.findIndex(c => c.id === focusedCellId);
    if (idx >= 0 && idx < cells.length - 1) setFocusedCell(cells[idx + 1].id);
  }, [focusedCellId]);

  // --- Variables ---

  const updateVariable = useCallback(
    (name: string, value: string) => {
      updateNotebook(nb => {
        const variable = nb.variables[name];
        if (!variable) return nb;

        const updatedVariables = {
          ...nb.variables,
          [name]: { ...variable, currentValue: value },
        };

        // Mark cells referencing this variable as stale
        const cells = nb.cells.map(cell => {
          if (cell.type !== 'sql' && cell.type !== 'mongo') return cell;
          if (cell.executionState !== 'success' && cell.executionState !== 'error') return cell;
          const refs = extractVariableReferences(cell.source);
          if (!refs.includes(name)) return cell;
          return { ...cell, executionState: 'stale' as CellExecutionState };
        });

        return { ...nb, variables: updatedVariables, cells };
      });
    },
    [updateNotebook]
  );

  const addVariable = useCallback(
    (variable: NotebookVariable) => {
      updateNotebook(nb => ({
        ...nb,
        variables: { ...nb.variables, [variable.name]: variable },
      }));
    },
    [updateNotebook]
  );

  const removeVariable = useCallback(
    (name: string) => {
      updateNotebook(nb => {
        const { [name]: _, ...rest } = nb.variables;
        return { ...nb, variables: rest };
      });
    },
    [updateNotebook]
  );

  // --- File operations ---

  const save = useCallback(async () => {
    try {
      const savedPath = await saveNotebookToFile(notebookRef.current, path);
      if (savedPath) {
        setPath(savedPath);
        setIsDirty(false);
        clearDraft(tabId);
        toast.success(t('notebook.saved'));
      }
    } catch {
      toast.error(t('notebook.saveError'));
    }
  }, [path, tabId, t]);

  const saveAs = useCallback(async () => {
    try {
      const savedPath = await saveNotebookToFile(notebookRef.current, null);
      if (savedPath) {
        setPath(savedPath);
        setIsDirty(false);
        clearDraft(tabId);
        toast.success(t('notebook.saved'));
      }
    } catch {
      toast.error(t('notebook.saveError'));
    }
  }, [tabId, t]);

  const openFromFile = useCallback(async () => {
    try {
      if (isDirty) {
        const confirmed = window.confirm(t('notebook.unsavedChanges'));
        if (!confirmed) return;
      }
      const result = await openNotebookFromFile();
      if (!result) return;
      setNotebook(result.notebook);
      setPath(result.path);
      setIsDirty(false);
      clearDraft(tabId);
      toast.success(t('notebook.open'));
    } catch {
      toast.error(t('notebook.openError'));
    }
  }, [isDirty, tabId, t]);

  const importFromFile = useCallback(async () => {
    try {
      const filePath = await openDialog({
        multiple: false,
        filters: [
          { name: 'SQL files', extensions: ['sql'] },
          { name: 'Markdown files', extensions: ['md'] },
        ],
      });
      if (!filePath || Array.isArray(filePath)) return;

      const content = await readTextFile(filePath);
      const isSql = filePath.endsWith('.sql');
      const fileName =
        filePath
          .split('/')
          .pop()
          ?.replace(/\.[^.]+$/, '') ?? 'Imported';
      const imported = isSql
        ? importFromSql(content, fileName)
        : importFromMarkdown(content, fileName);

      setNotebook(imported);
      setPath(null);
      setIsDirty(true);
      toast.success(t('notebook.importSuccess'));
    } catch {
      toast.error(t('notebook.openError'));
    }
  }, [t]);

  const exportToFile = useCallback(
    async (format: 'markdown' | 'html', includeResults = false) => {
      try {
        const nb = notebookRef.current;
        const ext = format === 'markdown' ? 'md' : 'html';
        const content =
          format === 'markdown'
            ? exportToMarkdown(nb, includeResults)
            : exportToHtml(nb, includeResults);

        const defaultName = nb.metadata.title.replace(/[^a-zA-Z0-9_-]/g, '_');
        const filePath = await saveDialog({
          defaultPath: `${defaultName}.${ext}`,
          filters: [{ name: `${format.toUpperCase()} file`, extensions: [ext] }],
        });
        if (!filePath) return;

        await writeTextFile(filePath, content);
        toast.success(t('notebook.exportSuccess'));
      } catch {
        toast.error(t('notebook.saveError'));
      }
    },
    [t]
  );

  // --- Metadata ---

  const setTitle = useCallback(
    (title: string) => {
      updateNotebook(nb => ({
        ...nb,
        metadata: { ...nb.metadata, title },
      }));
    },
    [updateNotebook]
  );

  return {
    notebook,
    path,
    isDirty,
    focusedCellId,
    isExecuting: executingCellId !== null,
    executingCellId,
    addCell,
    deleteCell,
    moveCellUp,
    moveCellDown,
    reorderCells,
    updateCellSource,
    setFocusedCell,
    executeCell,
    executeAll,
    executeFromHere,
    cancelExecution,
    clearAllResults,
    duplicateCell,
    convertCellType,
    toggleCellCollapsed,
    focusPrevCell,
    focusNextCell,
    updateVariable,
    addVariable,
    removeVariable,
    undo,
    redo,
    canUndo: history.canUndo,
    canRedo: history.canRedo,
    save,
    saveAs,
    openFromFile,
    importFromFile,
    exportToFile,
    setTitle,
  };
}
