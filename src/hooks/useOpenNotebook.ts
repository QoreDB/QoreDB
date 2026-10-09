// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { captureWorkspaceScope } from '../lib/stores/workspaceStore';
import { createNotebookTab, type OpenTab } from '../lib/tabs';

/** Shared by the file menu and command palette so both obey the same lifetime. */
export function useOpenNotebook(sessionId: string | null, openTab: (tab: OpenTab) => void) {
  const { t } = useTranslation();
  const context = useMemo(() => ({ sessionId, busy: false }), [sessionId]);
  const current = useRef<typeof context | null>(context);
  current.current = context;
  useEffect(() => {
    current.current = context;
    return () => {
      current.current = null;
    };
  }, [context]);

  return useCallback(async () => {
    const inWorkspace = captureWorkspaceScope();
    const isCurrent = () => current.current === context && inWorkspace();
    if (!context.sessionId || context.busy || !isCurrent()) return;
    context.busy = true;
    try {
      const { openNotebookFromFile, setPendingNotebook } = await import(
        '../lib/notebook/notebookIO'
      );
      if (!isCurrent()) return;
      const result = await openNotebookFromFile();
      if (!result || !isCurrent()) return;
      setPendingNotebook(result.path, result.notebook);
      openTab(createNotebookTab(result.notebook.metadata.title, result.path));
    } catch {
      if (isCurrent()) toast.error(t('notebook.openError'));
    } finally {
      context.busy = false;
    }
  }, [context, openTab, t]);
}
