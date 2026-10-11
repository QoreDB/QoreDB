// SPDX-License-Identifier: Apache-2.0

import { createContext, type ReactNode, useCallback, useContext, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { emitUiEvent, UI_EVENT_WORKSPACE_CHANGED } from '@/lib/events/uiEvents';
import { loadMigrations } from '@/lib/migrations/migrationsStore';
import { flushWorkspaceLibrary, syncWorkspaceLibrary } from '@/lib/query/queryLibrary';
import {
  setActiveWorkspace,
  setRecentWorkspaces,
  setWorkspaceLoading,
  useWorkspaceStore,
} from '@/lib/stores/workspaceStore';
import {
  detectWorkspace,
  getActiveWorkspace,
  getWorkspaceProjectId,
  listRecentWorkspaces,
  type RecentWorkspace,
  switchToDefaultWorkspace,
  createWorkspace as tauriCreateWorkspace,
  openWorkspace as tauriOpenWorkspace,
  switchWorkspace as tauriSwitchWorkspace,
  type WorkspaceInfo,
} from '@/lib/tauri';
import { isWeb, listen, type UnlistenFn } from '@/lib/transport';

const DISMISSED_WORKSPACE_KEY = 'qoredb_dismissed_workspaces';

function getDismissedWorkspaces(): Set<string> {
  try {
    const raw = localStorage.getItem(DISMISSED_WORKSPACE_KEY);
    if (!raw) return new Set();
    const arr = JSON.parse(raw);
    return new Set(Array.isArray(arr) ? arr : []);
  } catch {
    return new Set();
  }
}

export function addDismissedWorkspace(path: string) {
  const set = getDismissedWorkspaces();
  set.add(path);
  localStorage.setItem(DISMISSED_WORKSPACE_KEY, JSON.stringify([...set]));
}

function removeDismissedWorkspace(path: string) {
  const set = getDismissedWorkspaces();
  set.delete(path);
  localStorage.setItem(DISMISSED_WORKSPACE_KEY, JSON.stringify([...set]));
}

async function reloadWorkspaceLibrary() {
  try {
    await syncWorkspaceLibrary();
  } catch {
    console.warn('Failed to sync workspace library; local changes retained.');
  }
}

export interface WorkspaceContextValue {
  activeWorkspace: WorkspaceInfo | null;
  recentWorkspaces: RecentWorkspace[];
  projectId: string;
  isLoading: boolean;
  switchWorkspace: (qoredbPath: string) => Promise<boolean>;
  switchToDefault: () => Promise<void>;
  createWorkspace: (projectDir: string, name: string) => Promise<boolean>;
  openWorkspace: (qoredbPath: string) => Promise<boolean>;
  refreshRecents: () => Promise<void>;
}

const WorkspaceContext = createContext<WorkspaceContextValue | null>(null);

export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const activeWorkspace = useWorkspaceStore(s => s.activeWorkspace);
  const recentWorkspaces = useWorkspaceStore(s => s.recentWorkspaces);
  const projectId = useWorkspaceStore(s => s.projectId);
  const isLoading = useWorkspaceStore(s => s.isLoading);

  const changingWorkspace = useRef(false);

  const beginWorkspaceChange = useCallback(async () => {
    if (changingWorkspace.current) return false;
    changingWorkspace.current = true;
    setWorkspaceLoading(true);
    try {
      await flushWorkspaceLibrary();
      return true;
    } catch {
      toast.error(t('library.saveError'));
      changingWorkspace.current = false;
      setWorkspaceLoading(false);
      return false;
    }
  }, [t]);

  const finishWorkspaceChange = useCallback(() => {
    changingWorkspace.current = false;
    setWorkspaceLoading(false);
  }, []);

  // Initialize: detect workspace from CWD, load active + recents
  useEffect(() => {
    let cancelled = false;

    async function init() {
      if (isWeb) {
        setWorkspaceLoading(false);
        return;
      }
      try {
        await flushWorkspaceLibrary();
        const detected = await detectWorkspace();

        if (cancelled) return;

        if (detected && detected.source === 'detected') {
          if (getDismissedWorkspaces().has(detected.path)) {
            const active = await switchToDefaultWorkspace();
            if (!cancelled) setActiveWorkspace(active, 'default');
          } else {
            // Workspace detected and accepted (detect_workspace already activates it)
            const pid = await getWorkspaceProjectId();
            if (!cancelled) {
              setActiveWorkspace(detected, pid);
              await reloadWorkspaceLibrary();
              toast.info(t('workspace.detected'), {
                description: detected.manifest.name,
                duration: 5000,
              });
            }
          }
        } else {
          // No workspace detected, use default
          const active = await getActiveWorkspace();
          const pid = await getWorkspaceProjectId();
          if (!cancelled) {
            setActiveWorkspace(active, pid);
            await reloadWorkspaceLibrary();
          }
        }

        const recents = await listRecentWorkspaces();
        if (!cancelled) setRecentWorkspaces(recents);
      } catch (err) {
        console.error('Failed to initialize workspace:', err);
        if (!cancelled) setWorkspaceLoading(false);
      }
    }

    init();
    return () => {
      cancelled = true;
    };
  }, [t]);

  // Listen for workspace file changes from the Rust watcher
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    let cancelled = false;

    // Reload query library when .qoredb/queries/ changes externally
    listen('workspace_fs:queries', () => {
      if (!cancelled) void reloadWorkspaceLibrary();
    }).then(fn => {
      if (cancelled) fn();
      else unlisteners.push(fn);
    });

    // Reload migrations when .qoredb/migrations/ changes externally
    listen('workspace_fs:migrations', () => {
      if (!cancelled) void loadMigrations();
    }).then(fn => {
      if (cancelled) fn();
      else unlisteners.push(fn);
    });

    // Reload workspace manifest when workspace.json changes externally
    listen('workspace_fs:manifest', async () => {
      if (cancelled) return;
      try {
        const active = await getActiveWorkspace();
        const pid = await getWorkspaceProjectId();
        setActiveWorkspace(active, pid);
      } catch {
        // ignore
      }
    }).then(fn => {
      if (cancelled) fn();
      else unlisteners.push(fn);
    });

    return () => {
      cancelled = true;
      for (const fn of unlisteners) fn();
    };
  }, []);

  const switchWorkspace = useCallback(
    async (qoredbPath: string): Promise<boolean> => {
      if (!(await beginWorkspaceChange())) return false;
      try {
        const result = await tauriSwitchWorkspace(qoredbPath);
        if (result.success && result.workspace) {
          const pid = await getWorkspaceProjectId();
          setActiveWorkspace(result.workspace, pid);
          emitUiEvent(UI_EVENT_WORKSPACE_CHANGED);
          await reloadWorkspaceLibrary();
          removeDismissedWorkspace(qoredbPath);
          const recents = await listRecentWorkspaces();
          setRecentWorkspaces(recents);
          return true;
        }
        if (result.error) toast.error(result.error);
        return false;
      } catch (err) {
        toast.error(t('common.unknownError'));
        console.error('Failed to switch workspace:', err);
        return false;
      } finally {
        finishWorkspaceChange();
      }
    },
    [beginWorkspaceChange, finishWorkspaceChange, t]
  );

  const switchToDefault = useCallback(async () => {
    if (!(await beginWorkspaceChange())) return;
    try {
      const info = await switchToDefaultWorkspace();
      setActiveWorkspace(info, 'default');
      emitUiEvent(UI_EVENT_WORKSPACE_CHANGED);
    } catch (err) {
      toast.error(t('common.unknownError'));
      console.error('Failed to switch to default workspace:', err);
    } finally {
      finishWorkspaceChange();
    }
  }, [beginWorkspaceChange, finishWorkspaceChange, t]);

  const createWorkspace = useCallback(
    async (projectDir: string, name: string): Promise<boolean> => {
      if (!(await beginWorkspaceChange())) return false;
      try {
        const result = await tauriCreateWorkspace(projectDir, name);
        if (result.success && result.workspace) {
          const pid = await getWorkspaceProjectId();
          setActiveWorkspace(result.workspace, pid);
          emitUiEvent(UI_EVENT_WORKSPACE_CHANGED);
          await reloadWorkspaceLibrary();
          const recents = await listRecentWorkspaces();
          setRecentWorkspaces(recents);
          toast.success(t('workspace.created'));
          return true;
        }
        if (result.error) toast.error(result.error);
        return false;
      } catch (err) {
        toast.error(t('common.unknownError'));
        console.error('Failed to create workspace:', err);
        return false;
      } finally {
        finishWorkspaceChange();
      }
    },
    [beginWorkspaceChange, finishWorkspaceChange, t]
  );

  const openWorkspace = useCallback(
    async (qoredbPath: string): Promise<boolean> => {
      if (!(await beginWorkspaceChange())) return false;
      try {
        const result = await tauriOpenWorkspace(qoredbPath);
        if (result.success && result.workspace) {
          const pid = await getWorkspaceProjectId();
          setActiveWorkspace(result.workspace, pid);
          emitUiEvent(UI_EVENT_WORKSPACE_CHANGED);
          await reloadWorkspaceLibrary();
          const recents = await listRecentWorkspaces();
          setRecentWorkspaces(recents);
          return true;
        }
        if (result.error) toast.error(result.error);
        return false;
      } catch (err) {
        toast.error(t('common.unknownError'));
        console.error('Failed to open workspace:', err);
        return false;
      } finally {
        finishWorkspaceChange();
      }
    },
    [beginWorkspaceChange, finishWorkspaceChange, t]
  );

  const refreshRecents = useCallback(async () => {
    try {
      const recents = await listRecentWorkspaces();
      setRecentWorkspaces(recents);
    } catch (err) {
      console.error('Failed to load recent workspaces:', err);
    }
  }, []);

  return (
    <WorkspaceContext.Provider
      value={{
        activeWorkspace,
        recentWorkspaces,
        projectId,
        isLoading,
        switchWorkspace,
        switchToDefault,
        createWorkspace,
        openWorkspace,
        refreshRecents,
      }}
    >
      {children}
    </WorkspaceContext.Provider>
  );
}

export function useWorkspace(): WorkspaceContextValue {
  const ctx = useContext(WorkspaceContext);
  if (!ctx) throw new Error('useWorkspace must be used within WorkspaceProvider');
  return ctx;
}
