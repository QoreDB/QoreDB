// SPDX-License-Identifier: Apache-2.0

import { useSyncExternalStore } from 'react';
import type { RecentWorkspace, WorkspaceInfo } from '../tauri';

// ============================================
// STATE
// ============================================

interface WorkspaceState {
  activeWorkspace: WorkspaceInfo | null;
  recentWorkspaces: RecentWorkspace[];
  projectId: string;
  isLoading: boolean;
}

let state: WorkspaceState = {
  activeWorkspace: null,
  recentWorkspaces: [],
  projectId: 'default',
  isLoading: true,
};

const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

function updateState(
  updater: Partial<WorkspaceState> | ((current: WorkspaceState) => Partial<WorkspaceState>)
): boolean {
  const patch = typeof updater === 'function' ? updater(state) : updater;
  const changed = (Object.keys(patch) as Array<keyof WorkspaceState>).some(
    key => !Object.is(state[key], patch[key])
  );
  if (!changed) return false;
  state = { ...state, ...patch };
  emit();
  return true;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

// ============================================
// READ (non-reactive)
// ============================================

export function getWorkspaceState(): WorkspaceState {
  return state;
}

// ============================================
// SETTERS
// ============================================

export function setActiveWorkspace(workspace: WorkspaceInfo | null, projectId: string) {
  updateState({ activeWorkspace: workspace, projectId, isLoading: false });
}

export function setRecentWorkspaces(recents: RecentWorkspace[]) {
  updateState({ recentWorkspaces: recents });
}

export function setWorkspaceLoading(loading: boolean) {
  updateState({ isLoading: loading });
}

// ============================================
// REACT HOOK
// ============================================

export function useWorkspaceStore<T>(selector: (state: WorkspaceState) => T): T {
  return useSyncExternalStore(
    subscribe,
    () => selector(state),
    () => selector(state)
  );
}
