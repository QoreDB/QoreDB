// SPDX-License-Identifier: Apache-2.0

import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { confirmDialog } from '@/lib/stores/confirmStore';
import type { Driver } from '../lib/connection/drivers';
import { loadContract, runContract } from '../lib/contracts';
import { exportToHtml, exportToMarkdown } from '../lib/notebook/notebookExport';
import { importFromMarkdown, importFromSql } from '../lib/notebook/notebookImport';
import {
  findUnavailableReference,
  invalidateNotebookDependents,
  resolveNotebookReferences,
} from '../lib/notebook/notebookInterCellRef';
import {
  clearDraft,
  consumePendingNotebook,
  loadDraft,
  openNotebookFromFile,
  saveDraft,
  saveNotebookToFile,
} from '../lib/notebook/notebookIO';
import {
  type CellExecutionState,
  type CellResult,
  type CellType,
  createCell,
  createEmptyNotebook,
  type NotebookCell,
  type NotebookVariable,
  type QoreNotebook,
} from '../lib/notebook/notebookTypes';
import { useWorkspaceStore } from '../lib/stores/workspaceStore';
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
  addCell: (type: CellType, afterCellId?: string, source?: string) => void;
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

function withoutResults(notebook: QoreNotebook): QoreNotebook {
  return {
    ...notebook,
    cells: notebook.cells.map(cell => ({
      ...cell,
      lastResult: undefined,
      executionState: 'idle',
      executionCount: 0,
      executedAt: undefined,
      executionTimeMs: undefined,
    })),
  };
}

export function useNotebook(options: UseNotebookOptions): UseNotebookReturn {
  const { tabId, sessionId, namespace, initialPath, initialQuery, onDirtyChange } = options;
  const { t } = useTranslation();
  const projectId = useWorkspaceStore(state => state.projectId);

  const restoredDraftRef = useRef(false);
  const [notebook, setNotebook] = useState<QoreNotebook>(() => {
    // Priority: provided notebook > pending (opened from file menu) > draft > new with initialQuery > empty
    if (options.initialNotebook) return withoutResults(options.initialNotebook);
    if (initialPath) {
      const pending = consumePendingNotebook(initialPath);
      if (pending) return withoutResults(pending);
    }
    const draft = loadDraft(tabId);
    if (draft) {
      restoredDraftRef.current = true;
      return withoutResults(draft);
    }
    if (initialQuery) {
      const nb = createEmptyNotebook();
      nb.cells[0].source = initialQuery;
      return nb;
    }
    return createEmptyNotebook();
  });

  const [path, setPath] = useState<string | null>(initialPath ?? null);
  const [isDirty, setDirtyState] = useState(restoredDraftRef.current);
  const dirtyRef = useRef(restoredDraftRef.current);
  const setIsDirty = useCallback((dirty: boolean) => {
    dirtyRef.current = dirty;
    setDirtyState(dirty);
  }, []);
  const fileOperationRef = useRef(false);
  const contextEpochRef = useRef(0);
  const [focusedCellId, setFocusedCell] = useState<string | null>(notebook.cells[0]?.id ?? null);
  const [executingCellId, setExecutingCellId] = useState<string | null>(null);

  const notebookRef = useRef(notebook);

  const onDirtyChangeRef = useRef(onDirtyChange);
  onDirtyChangeRef.current = onDirtyChange;

  const history = useNotebookHistory();
  const clearHistory = history.clear;

  // Abort controller for batch execution (Run All / Run From Here)
  const abortRef = useRef<AbortController | null>(null);
  // Track the current queryId for cancellation
  const activeExecutionRef = useRef<{ cellId: string; sessionId: string; queryId?: string } | null>(
    null
  );

  const stopExecution = useCallback(() => {
    abortRef.current?.abort();
    abortRef.current = null;
    const active = activeExecutionRef.current;
    activeExecutionRef.current = null;
    if (active?.queryId) cancelQuery(active.sessionId, active.queryId).catch(() => {});
    setExecutingCellId(null);
  }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: each context boundary invalidates results and pending work
  useEffect(() => {
    contextEpochRef.current++;
    // Results belong to one execution context, including its current safety policy.
    stopExecution();
    clearHistory();
    const next = withoutResults(notebookRef.current);
    notebookRef.current = next;
    setNotebook(next);
    return () => {
      contextEpochRef.current++;
      stopExecution();
    };
  }, [
    sessionId,
    namespace?.database,
    namespace?.schema,
    projectId,
    clearHistory,
    options.dialect,
    options.environment,
    options.readOnly,
    stopExecution,
  ]);

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
    return () => {
      clearInterval(interval);
      if (dirtyRef.current) saveDraft(tabId, notebookRef.current);
    };
  }, [tabId, isDirty]);

  // --- Helpers ---

  const updateNotebook = useCallback(
    (updater: (prev: QoreNotebook) => QoreNotebook) => {
      // Push current state for undo BEFORE updating (outside setNotebook to avoid nested setState)
      history.pushState(notebookRef.current);
      const prev = notebookRef.current;
      const next = invalidateNotebookDependents(prev, updater(prev));
      notebookRef.current = next;
      const active = activeExecutionRef.current;
      if (
        active &&
        (!next.cells.some(cell => cell.id === active.cellId) ||
          ['idle', 'stale'].includes(
            next.cells.find(cell => cell.id === active.cellId)?.executionState ?? 'idle'
          ))
      ) {
        stopExecution();
      }
      setNotebook(next);
      setIsDirty(true);
    },
    [history, stopExecution, setIsDirty]
  );

  const undo = useCallback(() => {
    const restored = history.undo(notebookRef.current);
    if (restored) {
      stopExecution();
      notebookRef.current = withoutResults(restored);
      setNotebook(notebookRef.current);
      setIsDirty(true);
    }
  }, [history, stopExecution, setIsDirty]);

  const redo = useCallback(() => {
    const restored = history.redo(notebookRef.current);
    if (restored) {
      stopExecution();
      notebookRef.current = withoutResults(restored);
      setNotebook(notebookRef.current);
      setIsDirty(true);
    }
  }, [history, stopExecution, setIsDirty]);

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
    (type: CellType, afterCellId?: string, source?: string) => {
      const cell = createCell(type, source);
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
      updateCell(cellId, cell => ({ ...cell, source }));
    },
    [updateCell]
  );

  // --- Execution ---

  const executeContractCell = useCallback(
    async (cellId: string, signal?: AbortSignal) => {
      const cell = notebookRef.current.cells.find(c => c.id === cellId);
      if (!cell || cell.type !== 'contract') return;
      const name = cell.source.trim();
      if (!name) return;
      if (!sessionId) {
        toast.error(t('query.noConnectionError'));
        return;
      }
      if (signal?.aborted) return;

      const execution = { cellId, sessionId };
      activeExecutionRef.current = execution;
      const isCurrent = () => activeExecutionRef.current === execution && !signal?.aborted;
      setExecutingCellId(cellId);
      setFocusedCell(cellId);
      updateCell(cellId, c => ({
        ...c,
        executionState: 'running' as CellExecutionState,
      }));

      const startTime = performance.now();
      try {
        const source = await loadContract(name);
        if (!isCurrent()) return;
        const run = await runContract(sessionId, source);
        if (!isCurrent()) return;

        const totalTimeMs = Math.round(performance.now() - startTime);
        const success = run.fail_count === 0 && run.error_count === 0;
        const result: CellResult = {
          type: 'contract',
          contractRun: run,
        };
        updateCell(cellId, c => ({
          ...c,
          lastResult: result,
          executionState: success ? 'success' : 'error',
          executionCount: (c.executionCount ?? 0) + 1,
          executedAt: new Date().toISOString(),
          executionTimeMs: totalTimeMs,
        }));
        if (!success) {
          throw new Error(t('contracts.errors.runFailed'));
        }
      } catch (err) {
        if (!isCurrent()) return;
        const errorMessage = err instanceof Error ? err.message : String(err);
        const current = notebookRef.current.cells.find(c => c.id === cellId);
        if (current?.executionState === 'running') {
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
        if (activeExecutionRef.current === execution) {
          activeExecutionRef.current = null;
          setExecutingCellId(null);
        }
      }
    },
    [sessionId, updateCell, t]
  );

  const executeSingleCell = useCallback(
    async (cellId: string, signal?: AbortSignal) => {
      if (!sessionId) {
        toast.error(t('query.noConnectionError'));
        return;
      }

      const cell = notebookRef.current.cells.find(c => c.id === cellId);
      if (!cell) return;
      if (cell.type === 'contract') {
        await executeContractCell(cellId, signal);
        return;
      }
      if (cell.type !== 'sql' && cell.type !== 'mongo') return;
      if (!cell.source.trim()) return;

      if (signal?.aborted) return;

      const queryId = crypto.randomUUID();
      const execution = { cellId, sessionId, queryId };
      activeExecutionRef.current = execution;
      const isCurrent = () => activeExecutionRef.current === execution && !signal?.aborted;
      setExecutingCellId(cellId);
      setFocusedCell(cellId);
      updateCell(cellId, c => ({
        ...c,
        executionState: 'running' as CellExecutionState,
      }));

      const startTime = performance.now();

      try {
        const unavailable = findUnavailableReference(
          cell.source,
          notebookRef.current.cells,
          cell.type
        );
        if (unavailable)
          throw new Error(t('notebook.referenceUnavailable', { reference: unavailable }));
        const cellNamespace = cell.config?.namespace ?? namespace ?? undefined;
        const resolvedSource = resolveNotebookReferences(
          cell.source,
          notebookRef.current.variables,
          notebookRef.current.cells
        );
        const response = await executeQuery(sessionId, resolvedSource, {
          namespace: cellNamespace,
          queryId,
          recordable: true,
        });

        if (!isCurrent()) return;

        const totalTimeMs = Math.round(performance.now() - startTime);

        if (response.success && response.result) {
          const result: CellResult = {
            type: 'table',
            columns: response.result.columns,
            rows: response.result.rows,
            totalRows: response.result.rows.length,
            truncated: response.truncated,
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
        if (!isCurrent()) return;

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
        if (activeExecutionRef.current === execution) {
          activeExecutionRef.current = null;
          setExecutingCellId(null);
        }
      }
    },
    [sessionId, namespace, updateCell, t, executeContractCell]
  );

  const executeCell = useCallback(
    async (cellId: string) => {
      if (activeExecutionRef.current || abortRef.current) return;
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
      if (activeExecutionRef.current || abortRef.current) return;
      if (!sessionId) {
        toast.error(t('query.noConnectionError'));
        return;
      }

      const abort = new AbortController();
      abortRef.current = abort;
      let failed = false;

      try {
        for (const cellId of cellIds) {
          if (abort.signal.aborted) break;

          const cell = notebookRef.current.cells.find(c => c.id === cellId);
          if (!cell) continue;
          if (cell.type !== 'sql' && cell.type !== 'mongo' && cell.type !== 'contract') continue;
          if (!cell.source.trim()) continue;

          try {
            await executeSingleCell(cellId, abort.signal);
          } catch {
            failed = true;
            if (!continueOnError) break;
          }
        }

        if (!abort.signal.aborted && !failed) {
          toast.success(t('notebook.allCellsExecuted'));
        }
      } finally {
        if (abortRef.current === abort) abortRef.current = null;
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
    const currentCellId = activeExecutionRef.current?.cellId;
    stopExecution();
    if (currentCellId) {
      updateCell(currentCellId, c => ({
        ...c,
        executionState: 'error',
        lastResult: { type: 'error', error: t('notebook.executionStopped') },
      }));
      setExecutingCellId(null);
    }

    toast.info(t('notebook.executionStopped'));
  }, [stopExecution, updateCell, t]);

  const clearAllResults = useCallback(() => {
    stopExecution();
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
  }, [updateNotebook, stopExecution]);

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

        return { ...nb, variables: updatedVariables };
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

  const saveToPath = useCallback(
    async (targetPath: string | null) => {
      if (fileOperationRef.current) return;
      fileOperationRef.current = true;
      const snapshot = notebookRef.current;
      const epoch = contextEpochRef.current;
      try {
        const savedPath = await saveNotebookToFile(snapshot, targetPath);
        if (!savedPath || contextEpochRef.current !== epoch) return;
        setPath(savedPath);
        if (notebookRef.current === snapshot) {
          setIsDirty(false);
          clearDraft(tabId);
        } else {
          // The file contains the captured revision; newer edits still need saving.
          saveDraft(tabId, notebookRef.current);
        }
        toast.success(t('notebook.saved'));
      } catch {
        if (contextEpochRef.current === epoch) toast.error(t('notebook.saveError'));
      } finally {
        fileOperationRef.current = false;
      }
    },
    [tabId, t, setIsDirty]
  );

  const save = useCallback(() => saveToPath(path), [path, saveToPath]);
  const saveAs = useCallback(() => saveToPath(null), [saveToPath]);

  const replaceFromFile = useCallback(
    async (importing: boolean) => {
      if (fileOperationRef.current) return;
      fileOperationRef.current = true;
      const snapshot = notebookRef.current;
      const epoch = contextEpochRef.current;
      try {
        if (dirtyRef.current) {
          const confirmed = await confirmDialog({ description: t('notebook.unsavedChanges') });
          if (!confirmed) return;
        }
        let replacement: QoreNotebook;
        let replacementPath: string | null;
        if (importing) {
          const filePath = await openDialog({
            multiple: false,
            filters: [
              { name: 'SQL files', extensions: ['sql'] },
              { name: 'Markdown files', extensions: ['md'] },
            ],
          });
          if (!filePath || Array.isArray(filePath)) return;
          const content = await readTextFile(filePath);
          const fileName =
            filePath
              .split(/[/\\]/)
              .pop()
              ?.replace(/\.[^.]+$/, '') ?? 'Imported';
          replacement = filePath.toLowerCase().endsWith('.sql')
            ? importFromSql(content, fileName)
            : importFromMarkdown(content, fileName);
          replacementPath = null;
        } else {
          const result = await openNotebookFromFile();
          if (!result) return;
          replacement = result.notebook;
          replacementPath = result.path;
        }
        if (contextEpochRef.current !== epoch) return;
        if (notebookRef.current !== snapshot) {
          toast.error(t('notebook.unsavedChanges'));
          return;
        }
        stopExecution();
        clearHistory();
        const next = withoutResults(replacement);
        notebookRef.current = next;
        setNotebook(next);
        setPath(replacementPath);
        setFocusedCell(next.cells[0]?.id ?? null);
        setIsDirty(importing);
        if (importing) saveDraft(tabId, next);
        else clearDraft(tabId);
        toast.success(t(importing ? 'notebook.importSuccess' : 'notebook.open'));
      } catch {
        if (contextEpochRef.current === epoch) toast.error(t('notebook.openError'));
      } finally {
        fileOperationRef.current = false;
      }
    },
    [tabId, t, clearHistory, stopExecution, setIsDirty]
  );

  const openFromFile = useCallback(() => replaceFromFile(false), [replaceFromFile]);
  const importFromFile = useCallback(() => replaceFromFile(true), [replaceFromFile]);

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
