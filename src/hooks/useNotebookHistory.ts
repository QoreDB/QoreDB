// SPDX-License-Identifier: Apache-2.0

import { useCallback, useMemo, useRef, useState } from 'react';
import type { QoreNotebook } from '../lib/notebook/notebookTypes';

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
  clear: () => void;
  canUndo: boolean;
  canRedo: boolean;
}

export function useNotebookHistory(): UseNotebookHistoryReturn {
  const [state, setState] = useState<NotebookHistoryState>({
    undoStack: [],
    redoStack: [],
  });

  const stateRef = useRef(state);
  const publish = useCallback((next: NotebookHistoryState) => {
    stateRef.current = next;
    setState(next);
  }, []);
  const lastPushTime = useRef(0);

  const pushState = useCallback(
    (notebook: QoreNotebook) => {
      const now = Date.now();
      const prev = stateRef.current;
      if (now - lastPushTime.current < DEBOUNCE_MS) {
        if (prev.redoStack.length) publish({ ...prev, redoStack: [] });
        return;
      }
      lastPushTime.current = now;
      publish({ undoStack: [...prev.undoStack, notebook].slice(-MAX_HISTORY), redoStack: [] });
    },
    [publish]
  );

  const undo = useCallback(
    (current: QoreNotebook): QoreNotebook | null => {
      const prev = stateRef.current;
      const restored = prev.undoStack[prev.undoStack.length - 1];
      if (!restored) return null;
      lastPushTime.current = 0;
      publish({ undoStack: prev.undoStack.slice(0, -1), redoStack: [...prev.redoStack, current] });
      return restored;
    },
    [publish]
  );

  const redo = useCallback(
    (current: QoreNotebook): QoreNotebook | null => {
      const prev = stateRef.current;
      const restored = prev.redoStack[prev.redoStack.length - 1];
      if (!restored) return null;
      lastPushTime.current = 0;
      publish({ undoStack: [...prev.undoStack, current], redoStack: prev.redoStack.slice(0, -1) });
      return restored;
    },
    [publish]
  );

  const clear = useCallback(() => {
    lastPushTime.current = 0;
    publish({ undoStack: [], redoStack: [] });
  }, [publish]);

  return useMemo(
    () => ({
      pushState,
      undo,
      redo,
      clear,
      canUndo: state.undoStack.length > 0,
      canRedo: state.redoStack.length > 0,
    }),
    [pushState, undo, redo, clear, state.undoStack.length, state.redoStack.length]
  );
}
