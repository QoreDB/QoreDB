// SPDX-License-Identifier: Apache-2.0

// Persists session state to localStorage so it can be restored on app restart.

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

export function getSessionState(): AppSession {
  try {
    const data = localStorage.getItem(STORAGE_KEY);
    if (!data) return { activeSessionId: null, sessions: [] };
    return JSON.parse(data) as AppSession;
  } catch {
    return { activeSessionId: null, sessions: [] };
  }
}

export function saveSessionState(state: AppSession): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
}

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

export function removeSession(sessionId: string): void {
  const state = getSessionState();
  state.sessions = state.sessions.filter(s => s.sessionId !== sessionId);
  if (state.activeSessionId === sessionId) {
    state.activeSessionId = state.sessions[0]?.sessionId || null;
  }
  saveSessionState(state);
}

export function setActiveSession(sessionId: string | null): void {
  const state = getSessionState();
  state.activeSessionId = sessionId;
  saveSessionState(state);
}

export function updateSessionQuery(sessionId: string, query: string): void {
  const state = getSessionState();
  const session = state.sessions.find(s => s.sessionId === sessionId);
  if (session) {
    session.query = query;
    session.lastUsed = Date.now();
    saveSessionState(state);
  }
}

export function clearSessions(): void {
  localStorage.removeItem(STORAGE_KEY);
}
