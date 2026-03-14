// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import type { Driver } from '../lib/drivers';
import { clearDraft, loadDraft, saveDraft, saveNotebookToFile } from '../lib/notebookIO';
import {
  createCell,
  createEmptyNotebook,
  type CellExecutionState,
  type CellResult,
  type CellType,
  type NotebookCell,
  type QoreNotebook,
} from '../lib/notebookTypes';
import { executeQuery, type Namespace } from '../lib/tauri';

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
}

export interface UseNotebookReturn {
  notebook: QoreNotebook;
  path: string | null;
  isDirty: boolean;
  focusedCellId: string | null;
  isExecuting: boolean;
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
  // File operations
  save: () => Promise<void>;
  saveAs: () => Promise<void>;
  // Metadata
  setTitle: (title: string) => void;
}

export function useNotebook(options: UseNotebookOptions): UseNotebookReturn {
  const { tabId, sessionId, namespace, initialPath, initialQuery } = options;
  const { t } = useTranslation();

  const [notebook, setNotebook] = useState<QoreNotebook>(() => {
    // Priority: provided notebook > draft > new with initialQuery > empty
    if (options.initialNotebook) return options.initialNotebook;
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

  const updateNotebook = useCallback((updater: (prev: QoreNotebook) => QoreNotebook) => {
    setNotebook(prev => {
      const next = updater(prev);
      setIsDirty(true);
      return next;
    });
  }, []);

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
        const isStale = cell.executionState === 'success' || cell.executionState === 'error';
        return {
          ...cell,
          source,
          executionState: isStale ? ('idle' as const) : cell.executionState,
        };
      });
    },
    [updateCell]
  );

  // --- Execution ---

  const executeCell = useCallback(
    async (cellId: string) => {
      if (!sessionId) {
        toast.error(t('query.noConnectionError'));
        return;
      }

      const cell = notebookRef.current.cells.find(c => c.id === cellId);
      if (!cell || (cell.type !== 'sql' && cell.type !== 'mongo')) return;
      if (!cell.source.trim()) return;

      setExecutingCellId(cellId);
      updateCell(cellId, c => ({
        ...c,
        executionState: 'running' as CellExecutionState,
      }));

      const startTime = performance.now();

      try {
        const cellNamespace = cell.config?.namespace ?? namespace ?? undefined;
        const response = await executeQuery(sessionId, cell.source, {
          namespace: cellNamespace,
        });

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
        }
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : String(err);
        updateCell(cellId, c => ({
          ...c,
          lastResult: { type: 'error', error: errorMessage },
          executionState: 'error',
          executionCount: (c.executionCount ?? 0) + 1,
          executedAt: new Date().toISOString(),
          executionTimeMs: Math.round(performance.now() - startTime),
        }));
      } finally {
        setExecutingCellId(null);
      }
    },
    [sessionId, namespace, updateCell, t]
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
    addCell,
    deleteCell,
    moveCellUp,
    moveCellDown,
    reorderCells,
    updateCellSource,
    setFocusedCell,
    executeCell,
    save,
    saveAs,
    setTitle,
  };
}
