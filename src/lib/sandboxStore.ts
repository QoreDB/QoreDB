/**
 * Sandbox Store
 *
 * Manages sandbox state and preferences with localStorage persistence.
 * Pattern follows sessionStore.ts conventions.
 */

import {
  SandboxState,
  SandboxSession,
  SandboxChange,
  SandboxPreferences,
  SandboxChangeGroup,
} from './sandboxTypes';
import { Namespace, Value, RowData, TableSchema } from './tauri';

// Storage keys
const STATE_KEY = 'qoredb_sandbox_state';
const PREFS_KEY = 'qoredb_sandbox_prefs';
const PREFS_EVENT = 'qoredb:sandbox-prefs';
const BACKUP_KEY = 'qoredb_sandbox_backup';

// Default preferences
const DEFAULT_PREFERENCES: SandboxPreferences = {
  deleteDisplay: 'strikethrough',
  confirmOnDiscard: true,
  autoCollapsePanel: false,
  panelPageSize: 100,
};

// Change listeners for reactive updates
type SandboxListener = (sessionId: string) => void;
const listeners: Set<SandboxListener> = new Set();

/**
 * Generate a unique ID for a change
 */
function generateChangeId(): string {
  return `change_${Date.now()}_${Math.random().toString(36).slice(2, 11)}`;
}

/**
 * Notify all listeners of a change
 */
function notifyListeners(sessionId: string): void {
  listeners.forEach(listener => {
    try {
      listener(sessionId);
    } catch {
      // Ignore listener errors
    }
  });
}

/**
 * Get the current sandbox state
 */
export function getSandboxState(): SandboxState {
  try {
    const data = localStorage.getItem(STATE_KEY);
    if (!data) return { sessions: {} };
    return JSON.parse(data) as SandboxState;
  } catch (error) {
    console.error('Failed to load sandbox state from localStorage:', error);
    return { sessions: {} };
  }
}

/**
 * Save sandbox state
 */
export function saveSandboxState(state: SandboxState): void {
  localStorage.setItem(STATE_KEY, JSON.stringify(state));
}

/**
 * Get sandbox preferences
 */
export function getSandboxPreferences(): SandboxPreferences {
  try {
    const data = localStorage.getItem(PREFS_KEY);
    if (!data) return { ...DEFAULT_PREFERENCES };
    const parsed = JSON.parse(data);
    const panelPageSize =
      typeof parsed.panelPageSize === 'number' && parsed.panelPageSize >= 20
        ? Math.floor(parsed.panelPageSize)
        : DEFAULT_PREFERENCES.panelPageSize;
    return { ...DEFAULT_PREFERENCES, ...parsed, panelPageSize };
  } catch (error) {
    console.error('Failed to load sandbox preferences from localStorage:', error);
    return { ...DEFAULT_PREFERENCES };
  }
}

/**
 * Save sandbox preferences
 */
export function setSandboxPreferences(prefs: Partial<SandboxPreferences>): void {
  const current = getSandboxPreferences();
  const updated = { ...current, ...prefs };
  if (typeof updated.panelPageSize === 'number' && updated.panelPageSize < 20) {
    updated.panelPageSize = 20;
  }
  localStorage.setItem(PREFS_KEY, JSON.stringify(updated));
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new CustomEvent(PREFS_EVENT, { detail: updated }));
  }
}

/**
 * Get or create a sandbox session for a session ID
 */
export function getSandboxSession(sessionId: string): SandboxSession {
  const state = getSandboxState();
  if (state.sessions[sessionId]) {
    return state.sessions[sessionId];
  }
  return {
    sessionId,
    isActive: false,
    activatedAt: 0,
    changes: [],
  };
}

/**
 * Check if sandbox is active for a session
 */
export function isSandboxActive(sessionId: string): boolean {
  const session = getSandboxSession(sessionId);
  return session.isActive;
}

/**
 * Activate sandbox mode for a session
 */
export function activateSandbox(sessionId: string): void {
  const state = getSandboxState();
  const existing = state.sessions[sessionId];

  state.sessions[sessionId] = {
    sessionId,
    isActive: true,
    activatedAt: Date.now(),
    changes: existing?.changes ?? [],
  };

  saveSandboxState(state);
  notifyListeners(sessionId);
}

/**
 * Deactivate sandbox mode for a session
 * Optionally clear changes
 */
export function deactivateSandbox(sessionId: string, clearChanges = false): void {
  const state = getSandboxState();
  const existing = state.sessions[sessionId];

  if (existing) {
    state.sessions[sessionId] = {
      ...existing,
      isActive: false,
      changes: clearChanges ? [] : existing.changes,
    };
    saveSandboxState(state);
    notifyListeners(sessionId);
  }
}

/**
 * Add a change to the sandbox
 */
export function addSandboxChange(change: Omit<SandboxChange, 'id' | 'timestamp'>): SandboxChange {
  const state = getSandboxState();
  const session = state.sessions[change.sessionId] ?? {
    sessionId: change.sessionId,
    isActive: true,
    activatedAt: Date.now(),
    changes: [],
  };

  const newChange: SandboxChange = {
    ...change,
    id: generateChangeId(),
    timestamp: Date.now(),
  };

  // For updates and deletes, check if we're modifying an already inserted row
  if (change.type === 'update' || change.type === 'delete') {
    const existingInsertIndex = session.changes.findIndex(
      c =>
        c.type === 'insert' &&
        c.tableName === change.tableName &&
        c.namespace.database === change.namespace.database &&
        c.namespace.schema === change.namespace.schema &&
        matchesPrimaryKey(c.newValues, change.primaryKey)
    );

    if (existingInsertIndex >= 0) {
      if (change.type === 'delete') {
        // If deleting an inserted row, just remove the insert
        session.changes.splice(existingInsertIndex, 1);
        state.sessions[change.sessionId] = session;
        saveSandboxState(state);
        notifyListeners(change.sessionId);
        return newChange; // Return a "ghost" change for API consistency
      } else {
        // If updating an inserted row, merge the updates into the insert
        const existingInsert = session.changes[existingInsertIndex];
        existingInsert.newValues = {
          ...existingInsert.newValues,
          ...change.newValues,
        };
        state.sessions[change.sessionId] = session;
        saveSandboxState(state);
        notifyListeners(change.sessionId);
        return existingInsert;
      }
    }

    // Check if we're updating a row that already has an update
    if (change.type === 'update') {
      const existingUpdateIndex = session.changes.findIndex(
        c =>
          c.type === 'update' &&
          c.tableName === change.tableName &&
          c.namespace.database === change.namespace.database &&
          c.namespace.schema === change.namespace.schema &&
          matchesPrimaryKey(c.primaryKey, change.primaryKey)
      );

      if (existingUpdateIndex >= 0) {
        // Merge updates
        const existingUpdate = session.changes[existingUpdateIndex];
        existingUpdate.newValues = {
          ...existingUpdate.newValues,
          ...change.newValues,
        };
        existingUpdate.timestamp = Date.now();
        state.sessions[change.sessionId] = session;
        saveSandboxState(state);
        notifyListeners(change.sessionId);
        return existingUpdate;
      }
    }
  }

  session.changes.push(newChange);
  state.sessions[change.sessionId] = session;
  saveSandboxState(state);
  notifyListeners(change.sessionId);

  return newChange;
}

/**
 * Check if two primary key objects match
 */
function matchesPrimaryKey(
  values: Record<string, Value> | RowData | undefined,
  primaryKey: RowData | undefined
): boolean {
  if (!values || !primaryKey?.columns) return false;

  const valueMap =
    (values as RowData).columns !== undefined
      ? (values as RowData).columns
      : (values as Record<string, Value>);

  const pkColumns = Object.keys(primaryKey.columns);
  return pkColumns.every(col => {
    const v1 = valueMap[col];
    const v2 = primaryKey.columns[col];
    
    // Use strict equality for primitives and same reference
    if (v1 === v2) return true;
    
    // Handle null/undefined cases
    if (v1 == null || v2 == null) return false;
    
    // For objects and arrays, use stable stringify for deep comparison
    if (typeof v1 === 'object' || typeof v2 === 'object') {
      return stableStringify(v1) === stableStringify(v2);
    }
    
    return false;
  });
}

function stableStringify(value: unknown): string {
  if (value === null) return 'null';
  if (Array.isArray(value)) {
    return `[${value.map(stableStringify).join(',')}]`;
  }
  if (typeof value === 'object') {
    const obj = value as Record<string, unknown>;
    const keys = Object.keys(obj).sort();
    return `{${keys.map(k => `${k}:${stableStringify(obj[k])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

/**
 * Remove a change from the sandbox by ID
 */
export function removeSandboxChange(sessionId: string, changeId: string): void {
  const state = getSandboxState();
  const session = state.sessions[sessionId];

  if (session) {
    session.changes = session.changes.filter(c => c.id !== changeId);
    saveSandboxState(state);
    notifyListeners(sessionId);
  }
}

/**
 * Get all changes for a specific table
 */
export function getChangesForTable(
  sessionId: string,
  namespace: Namespace,
  tableName: string
): SandboxChange[] {
  const session = getSandboxSession(sessionId);
  return session.changes.filter(
    c =>
      c.tableName === tableName &&
      c.namespace.database === namespace.database &&
      c.namespace.schema === namespace.schema
  );
}

/**
 * Get the count of changes for a session
 */
export function getChangesCount(sessionId: string): number {
  const session = getSandboxSession(sessionId);
  return session.changes.length;
}

/**
 * Clear all changes for a session
 */
export function clearSandboxChanges(sessionId: string): void {
  const state = getSandboxState();
  const session = state.sessions[sessionId];

  if (session) {
    session.changes = [];
    saveSandboxState(state);
    notifyListeners(sessionId);
  }
}

/**
 * Clear changes for a specific table
 */
export function clearTableChanges(
  sessionId: string,
  namespace: Namespace,
  tableName: string
): void {
  const state = getSandboxState();
  const session = state.sessions[sessionId];

  if (session) {
    session.changes = session.changes.filter(
      c =>
        !(
          c.tableName === tableName &&
          c.namespace.database === namespace.database &&
          c.namespace.schema === namespace.schema
        )
    );
    saveSandboxState(state);
    notifyListeners(sessionId);
  }
}

/**
 * Get changes grouped by table
 */
export function getGroupedChanges(sessionId: string): SandboxChangeGroup[] {
  const session = getSandboxSession(sessionId);
  const groups = new Map<string, SandboxChangeGroup>();

  for (const change of session.changes) {
    const key = `${change.namespace.database}:${change.namespace.schema ?? ''}:${change.tableName}`;

    if (!groups.has(key)) {
      const displayName = change.namespace.schema
        ? `${change.namespace.schema}.${change.tableName}`
        : change.tableName;

      groups.set(key, {
        namespace: change.namespace,
        tableName: change.tableName,
        displayName,
        changes: [],
        counts: { insert: 0, update: 0, delete: 0 },
      });
    }

    const group = groups.get(key)!;
    group.changes.push(change);
    group.counts[change.type]++;
  }

  // Sort by most recent change
  return Array.from(groups.values()).sort((a, b) => {
    const latestA = Math.max(...a.changes.map(c => c.timestamp));
    const latestB = Math.max(...b.changes.map(c => c.timestamp));
    return latestB - latestA;
  });
}

/**
 * Remove sandbox session when connection is closed
 */
export function removeSandboxSession(sessionId: string): void {
  const state = getSandboxState();
  delete state.sessions[sessionId];
  saveSandboxState(state);
}

/**
 * Subscribe to sandbox changes for a session
 * Returns an unsubscribe function
 */
export function subscribeSandbox(listener: SandboxListener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * Subscribe to sandbox preference changes
 */
export function subscribeSandboxPreferences(
  listener: (prefs: SandboxPreferences) => void
): () => void {
  const handler = (event: Event) => {
    const next = (event as CustomEvent<SandboxPreferences>).detail;
    if (next) {
      listener(next);
    }
  };
  window.addEventListener(PREFS_EVENT, handler as EventListener);
  return () => {
    window.removeEventListener(PREFS_EVENT, handler as EventListener);
  };
}

interface SandboxBackup {
  sessionId: string;
  isActive: boolean;
  changes: SandboxChange[];
  savedAt: number;
}

function getSandboxBackups(): Record<string, SandboxBackup> {
  try {
    const raw = localStorage.getItem(BACKUP_KEY);
    if (!raw) return {};
    return JSON.parse(raw) as Record<string, SandboxBackup>;
  } catch {
    return {};
  }
}

/**
 * Save a sandbox backup for a connection ID
 */
export function saveSandboxBackup(connectionId: string, sessionId: string): void {
  const session = getSandboxSession(sessionId);
  const backups = getSandboxBackups();
  
  // Use the most recent change timestamp, or current time if no changes
  const mostRecentChangeTime = session.changes.length > 0
    ? Math.max(...session.changes.map(change => change.timestamp))
    : Date.now();
  
  backups[connectionId] = {
    sessionId,
    isActive: session.isActive,
    changes: session.changes,
    savedAt: mostRecentChangeTime,
  };
  localStorage.setItem(BACKUP_KEY, JSON.stringify(backups));
}

/**
 * Get a sandbox backup for a connection ID
 */
export function getSandboxBackup(connectionId: string): SandboxBackup | null {
  const backups = getSandboxBackups();
  return backups[connectionId] ?? null;
}

/**
 * Clear a sandbox backup for a connection ID
 */
export function clearSandboxBackup(connectionId: string): void {
  const backups = getSandboxBackups();
  if (backups[connectionId]) {
    delete backups[connectionId];
    localStorage.setItem(BACKUP_KEY, JSON.stringify(backups));
  }
}

/**
 * Create an insert change
 */
export function createInsertChange(
  sessionId: string,
  namespace: Namespace,
  tableName: string,
  newValues: Record<string, Value>,
  schema?: TableSchema
): SandboxChange {
  return addSandboxChange({
    sessionId,
    type: 'insert',
    namespace,
    tableName,
    newValues,
    schema,
  });
}

/**
 * Create an update change
 */
export function createUpdateChange(
  sessionId: string,
  namespace: Namespace,
  tableName: string,
  primaryKey: RowData,
  oldValues: Record<string, Value>,
  newValues: Record<string, Value>,
  schema?: TableSchema
): SandboxChange {
  return addSandboxChange({
    sessionId,
    type: 'update',
    namespace,
    tableName,
    primaryKey,
    oldValues,
    newValues,
    schema,
  });
}

/**
 * Create a delete change
 */
export function createDeleteChange(
  sessionId: string,
  namespace: Namespace,
  tableName: string,
  primaryKey: RowData,
  oldValues?: Record<string, Value>,
  schema?: TableSchema
): SandboxChange {
  return addSandboxChange({
    sessionId,
    type: 'delete',
    namespace,
    tableName,
    primaryKey,
    oldValues,
    schema,
  });
}

/**
 * Check if there are pending changes that would be lost
 */
export function hasPendingChanges(sessionId: string): boolean {
  return getChangesCount(sessionId) > 0;
}

/**
 * Export all changes for a session (for debugging or backup)
 */
export function exportChanges(sessionId: string): SandboxChange[] {
  const session = getSandboxSession(sessionId);
  return [...session.changes];
}

/**
 * Import changes into a session (for restore)
 */
export function importChanges(sessionId: string, changes: SandboxChange[]): void {
  const state = getSandboxState();
  const session = state.sessions[sessionId] ?? {
    sessionId,
    isActive: true,
    activatedAt: Date.now(),
    changes: [],
  };

  // Reassign IDs to avoid conflicts
  const importedChanges = changes.map(c => ({
    ...c,
    id: generateChangeId(),
    sessionId,
  }));

  session.changes = [...session.changes, ...importedChanges];
  state.sessions[sessionId] = session;
  saveSandboxState(state);
  notifyListeners(sessionId);
}
