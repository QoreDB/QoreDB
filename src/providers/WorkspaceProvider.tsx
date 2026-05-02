// SPDX-License-Identifier: Apache-2.0

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { createContext, type ReactNode, useCallback, useContext, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { emitUiEvent, UI_EVENT_WORKSPACE_CHANGED } from '@/lib/events/uiEvents';
import { loadWorkspaceLibrary } from '@/lib/query/queryLibrary';
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
  wsGetQueryLibrary,
} from '@/lib/tauri';

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

/** Load the workspace query library from disk into localStorage */
async function syncWorkspaceLibrary() {
  try {
    const lib = await wsGetQueryLibrary();
    if (lib) {
      loadWorkspaceLibrary(lib);
    }
  } catch (err) {
    console.warn('Failed to sync workspace library:', err);
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

  // Initialize: detect workspace from CWD, load active + recents
  useEffect(() => {
    let cancelled = false;

    async function init() {
      try {
        // Try to detect a workspace from CWD
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
              await syncWorkspaceLibrary();
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
          if (!cancelled) setActiveWorkspace(active, pid);
        }

        // Load recent workspaces
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
      if (!cancelled) syncWorkspaceLibrary();
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
      try {
        const result = await tauriSwitchWorkspace(qoredbPath);
        if (result.success && result.workspace) {
          const pid = await getWorkspaceProjectId();
          setActiveWorkspace(result.workspace, pid);
          emitUiEvent(UI_EVENT_WORKSPACE_CHANGED);
          await syncWorkspaceLibrary();
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
      }
    },
    [t]
  );

  const switchToDefault = useCallback(async () => {
    try {
      const info = await switchToDefaultWorkspace();
      setActiveWorkspace(info, 'default');
      emitUiEvent(UI_EVENT_WORKSPACE_CHANGED);
    } catch (err) {
      console.error('Failed to switch to default workspace:', err);
    }
  }, []);

  const createWorkspace = useCallback(
    async (projectDir: string, name: string): Promise<boolean> => {
      try {
        const result = await tauriCreateWorkspace(projectDir, name);
        if (result.success && result.workspace) {
          const pid = await getWorkspaceProjectId();
          setActiveWorkspace(result.workspace, pid);
          emitUiEvent(UI_EVENT_WORKSPACE_CHANGED);
          await syncWorkspaceLibrary();
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
      }
    },
    [t]
  );

  const openWorkspace = useCallback(
    async (qoredbPath: string): Promise<boolean> => {
      try {
        const result = await tauriOpenWorkspace(qoredbPath);
        if (result.success && result.workspace) {
          const pid = await getWorkspaceProjectId();
          setActiveWorkspace(result.workspace, pid);
          emitUiEvent(UI_EVENT_WORKSPACE_CHANGED);
          await syncWorkspaceLibrary();
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
      }
    },
    [t]
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
