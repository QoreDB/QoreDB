// SPDX-License-Identifier: Apache-2.0

import { useCallback, useMemo, useRef, useState } from 'react';
import type { QoreNotebook } from '../lib/notebookTypes';

const MAX_HISTORY = 50;
const DEBOUNCE_MS = 500;

interface NotebookHistoryState {
  undoStack: QoreNotebook[];
  redoStack: QoreNotebook[];
}

export interface UseNotebookHistoryReturn {
  pushState: (notebook: QoreNotebook) => void;
  undo: (current: QoreNotebook) => QoreNotebook | null;
  redo: (current: QoreNotebook) => QoreNotebook | null;
  canUndo: boolean;
  canRedo: boolean;
}

export function useNotebookHistory(): UseNotebookHistoryReturn {
  const [state, setState] = useState<NotebookHistoryState>({
    undoStack: [],
    redoStack: [],
  });

  const lastPushTime = useRef(0);

  const pushState = useCallback((notebook: QoreNotebook) => {
    const now = Date.now();
    if (now - lastPushTime.current < DEBOUNCE_MS) return;
    lastPushTime.current = now;

    setState(prev => {
      const undoStack = [...prev.undoStack, notebook];
      if (undoStack.length > MAX_HISTORY) undoStack.shift();
      return { undoStack, redoStack: [] };
    });
  }, []);

  const undo = useCallback((current: QoreNotebook): QoreNotebook | null => {
    let result: QoreNotebook | null = null;
    setState(prev => {
      if (prev.undoStack.length === 0) return prev;
      const undoStack = [...prev.undoStack];
      const restored = undoStack.pop();
      if (!restored) return prev;
      result = restored;
      return {
        undoStack,
        redoStack: [...prev.redoStack, current],
      };
    });
    return result;
  }, []);

  const redo = useCallback((current: QoreNotebook): QoreNotebook | null => {
    let result: QoreNotebook | null = null;
    setState(prev => {
      if (prev.redoStack.length === 0) return prev;
      const redoStack = [...prev.redoStack];
      const restored = redoStack.pop();
      if (!restored) return prev;
      result = restored;
      return {
        undoStack: [...prev.undoStack, current],
        redoStack,
      };
    });
    return result;
  }, []);

  return useMemo(
    () => ({
      pushState,
      undo,
      redo,
      canUndo: state.undoStack.length > 0,
      canRedo: state.redoStack.length > 0,
    }),
    [pushState, undo, redo, state.undoStack.length, state.redoStack.length]
  );
}
