// SPDX-License-Identifier: Apache-2.0

import { useSyncExternalStore } from 'react';
import { UI_EVENT_OPEN_LOGS } from '../events/uiEvents';
import type { SavedConnection } from '../tauri';

interface ModalState {
  searchOpen: boolean;
  fulltextSearchOpen: boolean;
  connectionModalOpen: boolean;
  libraryModalOpen: boolean;
  logsOpen: boolean;
  auditLogOpen: boolean;
  contractsOpen: boolean;
  instantApiOpen: boolean;
  settingsOpen: boolean;
  /** Section the settings page should open on. Null defaults to General. */
  settingsSection: string | null;
  sidebarVisible: boolean;
  showOnboarding: boolean;
  cheatsheetOpen: boolean;
  zenMode: boolean;
  proDiscoveryOpen: boolean;
  whatsNewOpen: boolean;
  newsletterPromptOpen: boolean;
  editConnection: SavedConnection | null;
  editPassword: string;
  backupTarget: BackupTarget | null;
  restoreTarget: BackupTarget | null;
  importSqlTarget: ImportSqlTarget | null;
}

export interface BackupTarget {
  connection: SavedConnection;
  database: string | null;
}

export interface ImportSqlTarget {
  sessionId: string;
  database: string;
  schema: string | null;
  label: string;
}

let state: ModalState = {
  searchOpen: false,
  fulltextSearchOpen: false,
  connectionModalOpen: false,
  libraryModalOpen: false,
  logsOpen: false,
  auditLogOpen: false,
  contractsOpen: false,
  instantApiOpen: false,
  settingsOpen: false,
  settingsSection: null,
  sidebarVisible: true,
  showOnboarding: false,
  cheatsheetOpen: false,
  zenMode: false,
  proDiscoveryOpen: false,
  whatsNewOpen: false,
  newsletterPromptOpen: false,
  editConnection: null,
  editPassword: '',
  backupTarget: null,
  restoreTarget: null,
  importSqlTarget: null,
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

/** Non-reactive read, for use in event handlers. */
export function getModalState(): ModalState {
  return state;
}

export function setSearchOpen(open: boolean) {
  updateState({ searchOpen: open });
}

export function setFulltextSearchOpen(open: boolean) {
  updateState({ fulltextSearchOpen: open });
}

export function setConnectionModalOpen(open: boolean) {
  if (state.connectionModalOpen === open) return;
  updateState({ connectionModalOpen: open });
}

export function setLibraryModalOpen(open: boolean) {
  updateState({ libraryModalOpen: open });
}

export function setLogsOpen(open: boolean) {
  if (state.logsOpen === open) return;
  updateState({ logsOpen: open });
}

export function setAuditLogOpen(open: boolean) {
  if (state.auditLogOpen === open) return;
  updateState({ auditLogOpen: open });
}

export function setContractsOpen(open: boolean) {
  if (state.contractsOpen === open) return;
  updateState({ contractsOpen: open });
}

export function setInstantApiOpen(open: boolean) {
  if (state.instantApiOpen === open) return;
  updateState({ instantApiOpen: open });
}

export function openBackupDialog(connection: SavedConnection, database?: string | null) {
  updateState({ backupTarget: { connection, database: database ?? connection.database ?? null } });
}

export function closeBackupDialog() {
  updateState({ backupTarget: null });
}

export function openRestoreDialog(connection: SavedConnection, database?: string | null) {
  updateState({ restoreTarget: { connection, database: database ?? connection.database ?? null } });
}

export function closeRestoreDialog() {
  updateState({ restoreTarget: null });
}

export function openImportSqlDialog(target: ImportSqlTarget) {
  updateState({ importSqlTarget: target });
}

export function closeImportSqlDialog() {
  updateState({ importSqlTarget: null });
}

export function setSettingsOpen(open: boolean) {
  updateState({ settingsOpen: open, settingsSection: open ? state.settingsSection : null });
}

/** Opens the settings page directly on a given section. */
export function openSettingsSection(section: string) {
  updateState({ settingsOpen: true, settingsSection: section });
}

export function setProDiscoveryOpen(open: boolean) {
  if (state.proDiscoveryOpen === open) return;
  updateState({ proDiscoveryOpen: open });
}

export function setWhatsNewOpen(open: boolean) {
  if (state.whatsNewOpen === open) return;
  updateState({ whatsNewOpen: open });
}

export function setNewsletterPromptOpen(open: boolean) {
  if (state.newsletterPromptOpen === open) return;
  updateState({ newsletterPromptOpen: open });
}

export function setSidebarVisible(visible: boolean) {
  updateState({ sidebarVisible: visible });
}

export function setShowOnboarding(show: boolean) {
  updateState({ showOnboarding: show });
}

export function setCheatsheetOpen(open: boolean) {
  updateState({ cheatsheetOpen: open });
}

export function toggleCheatsheet() {
  updateState(currentState => ({ cheatsheetOpen: !currentState.cheatsheetOpen }));
}

export function setZenMode(mode: boolean) {
  updateState({ zenMode: mode });
}

export function toggleZenMode() {
  updateState(currentState => ({ zenMode: !currentState.zenMode }));
}

export function handleEditConnection(connection: SavedConnection, password: string) {
  updateState({ editConnection: connection, editPassword: password, connectionModalOpen: true });
}

export function handleCloseConnectionModal() {
  updateState({ connectionModalOpen: false, editConnection: null, editPassword: '' });
}

export function toggleSidebar() {
  updateState(currentState => ({ sidebarVisible: !currentState.sidebarVisible }));
}

/**
 * Subscribe to a specific slice of modal state. The component only re-renders
 * when the selected value changes. For objects, avoid inline selectors that
 * return a new object each time.
 */
export function useModalStore<T>(selector: (state: ModalState) => T): T {
  return useSyncExternalStore(
    subscribe,
    () => selector(state),
    () => selector(state)
  );
}

if (typeof window !== 'undefined') {
  window.addEventListener(UI_EVENT_OPEN_LOGS, () => setLogsOpen(true));
}
