// SPDX-License-Identifier: Apache-2.0

import { useSyncExternalStore } from 'react';
import type { SavedConnection } from './tauri';
import { UI_EVENT_OPEN_LOGS } from './uiEvents';

// ============================================
// STATE
// ============================================

interface ModalState {
  searchOpen: boolean;
  fulltextSearchOpen: boolean;
  connectionModalOpen: boolean;
  libraryModalOpen: boolean;
  logsOpen: boolean;
  settingsOpen: boolean;
  sidebarVisible: boolean;
  showOnboarding: boolean;
  editConnection: SavedConnection | null;
  editPassword: string;
}

let state: ModalState = {
  searchOpen: false,
  fulltextSearchOpen: false,
  connectionModalOpen: false,
  libraryModalOpen: false,
  logsOpen: false,
  settingsOpen: false,
  sidebarVisible: true,
  showOnboarding: false,
  editConnection: null,
  editPassword: '',
};

const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

function updateState(
  updater: Partial<ModalState> | ((currentState: ModalState) => Partial<ModalState>)
): boolean {
  const patch = typeof updater === 'function' ? updater(state) : updater;
  const changed = (Object.keys(patch) as Array<keyof ModalState>).some(
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
// READ (non-reactive, for use in event handlers)
// ============================================

export function getModalState(): ModalState {
  return state;
}

// ============================================
// SETTERS
// ============================================

export function setSearchOpen(open: boolean) {
  updateState({ searchOpen: open });
}

export function setFulltextSearchOpen(open: boolean) {
  updateState({ fulltextSearchOpen: open });
}

export function setConnectionModalOpen(open: boolean) {
  if (state.connectionModalOpen === open) return;
  if (import.meta.env.DEV) console.trace(`[modalStore] setConnectionModalOpen(${open})`);
  updateState({ connectionModalOpen: open });
}

export function setLibraryModalOpen(open: boolean) {
  updateState({ libraryModalOpen: open });
}

export function setLogsOpen(open: boolean) {
  if (state.logsOpen === open) return;
  if (import.meta.env.DEV) console.trace(`[modalStore] setLogsOpen(${open})`);
  updateState({ logsOpen: open });
}

export function setSettingsOpen(open: boolean) {
  updateState({ settingsOpen: open });
}

export function setSidebarVisible(visible: boolean) {
  updateState({ sidebarVisible: visible });
}

export function setShowOnboarding(show: boolean) {
  updateState({ showOnboarding: show });
}

// ============================================
// COMPOSITE ACTIONS
// ============================================

export function handleEditConnection(connection: SavedConnection, password: string) {
  updateState({ editConnection: connection, editPassword: password, connectionModalOpen: true });
}

export function handleCloseConnectionModal() {
  if (import.meta.env.DEV) console.trace('[modalStore] handleCloseConnectionModal()');
  updateState({ connectionModalOpen: false, editConnection: null, editPassword: '' });
}

export function toggleSidebar() {
  updateState(currentState => ({ sidebarVisible: !currentState.sidebarVisible }));
}

// ============================================
// REACT HOOKS (selector-based, granular re-renders)
// ============================================

/**
 * Subscribe to a specific slice of modal state.
 * The component only re-renders when the selected value changes.
 *
 * For primitives (boolean, string): works out of the box.
 * For objects: avoid inline selectors that return new objects each time.
 */
export function useModalStore<T>(selector: (state: ModalState) => T): T {
  return useSyncExternalStore(
    subscribe,
    () => selector(state),
    () => selector(state)
  );
}

// ============================================
// GLOBAL EVENT LISTENERS
// ============================================

if (typeof window !== 'undefined') {
  window.addEventListener(UI_EVENT_OPEN_LOGS, () => setLogsOpen(true));
}
