// SPDX-License-Identifier: Apache-2.0

/**
 * Session Store
 *
 * Persists the current session state for restoring on app restart.
 */

export interface SavedSession {
  sessionId: string;
  connectionId: string;
  driver: string;
  query: string;
  lastUsed: number;
}

export interface AppSession {
  activeSessionId: string | null;
  sessions: SavedSession[];
}

const STORAGE_KEY = 'qoredb_session_state';

/**
 * Get saved session state
 */
export function getSessionState(): AppSession {
  try {
    const data = localStorage.getItem(STORAGE_KEY);
    if (!data) return { activeSessionId: null, sessions: [] };
    return JSON.parse(data) as AppSession;
  } catch {
    return { activeSessionId: null, sessions: [] };
  }
}

/**
 * Save session state
 */
export function saveSessionState(state: AppSession): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
}

/**
 * Add or update a session
 */
export function saveSession(session: SavedSession): void {
  const state = getSessionState();

  const existingIndex = state.sessions.findIndex(s => s.sessionId === session.sessionId);
  if (existingIndex >= 0) {
    state.sessions[existingIndex] = session;
  } else {
    state.sessions.push(session);
  }

  saveSessionState(state);
}

/**
 * Remove a session
 */
export function removeSession(sessionId: string): void {
  const state = getSessionState();
  state.sessions = state.sessions.filter(s => s.sessionId !== sessionId);
  if (state.activeSessionId === sessionId) {
    state.activeSessionId = state.sessions[0]?.sessionId || null;
  }
  saveSessionState(state);
}

/**
 * Set the active session
 */
export function setActiveSession(sessionId: string | null): void {
  const state = getSessionState();
  state.activeSessionId = sessionId;
  saveSessionState(state);
}

/**
 * Update session query
 */
export function updateSessionQuery(sessionId: string, query: string): void {
  const state = getSessionState();
  const session = state.sessions.find(s => s.sessionId === sessionId);
  if (session) {
    session.query = query;
    session.lastUsed = Date.now();
    saveSessionState(state);
  }
}

/**
 * Clear all sessions
 */
export function clearSessions(): void {
  localStorage.removeItem(STORAGE_KEY);
}
