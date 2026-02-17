// SPDX-License-Identifier: Apache-2.0

/**
 * Query History Store
 *
 * Persists query history to localStorage with session isolation.
 */

import { shouldStoreHistory } from './diagnosticsSettings';
import { redactQuery, redactText } from './redaction';

export interface HistoryEntry {
  id: string;
  query: string;
  sessionId: string;
  driver: string;
  database?: string;
  executedAt: number; // timestamp
  executionTimeMs?: number;
  totalTimeMs?: number;
  rowCount?: number;
  error?: string;
}

const STORAGE_KEY = 'qoredb_query_history';
const MAX_ENTRIES = 100;
const MAX_IN_MEMORY = 100;

let inMemoryHistory: HistoryEntry[] = [];

function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

/**
 * Get all history entries
 */
export function getHistory(): HistoryEntry[] {
  if (!shouldStoreHistory()) {
    return inMemoryHistory;
  }
  try {
    const data = localStorage.getItem(STORAGE_KEY);
    if (!data) return [];
    return JSON.parse(data) as HistoryEntry[];
  } catch {
    return [];
  }
}

/**
 * Add a new entry to history
 */
export function addToHistory(entry: Omit<HistoryEntry, 'id'>): HistoryEntry {
  const history = getHistory();

  const newEntry: HistoryEntry = {
    ...entry,
    id: generateId(),
    query: shouldStoreHistory() ? redactQuery(entry.query) : entry.query,
    error: entry.error ? redactText(entry.error) : undefined,
  };

  // Add to beginning
  history.unshift(newEntry);

  if (shouldStoreHistory()) {
    // Trim to max entries
    if (history.length > MAX_ENTRIES) {
      history.splice(MAX_ENTRIES);
    }

    localStorage.setItem(STORAGE_KEY, JSON.stringify(history));
  } else {
    if (history.length > MAX_IN_MEMORY) {
      history.splice(MAX_IN_MEMORY);
    }
    inMemoryHistory = history;
  }

  return newEntry;
}

/**
 * Get history entries for a specific session
 */
export function getSessionHistory(sessionId: string): HistoryEntry[] {
  return getHistory().filter(e => e.sessionId === sessionId);
}

/**
 * Search history entries
 */
export function searchHistory(query: string): HistoryEntry[] {
  const lowerQuery = query.toLowerCase();
  return getHistory().filter(e => e.query.toLowerCase().includes(lowerQuery));
}

/**
 * Clear all history
 */
export function clearHistory(): void {
  inMemoryHistory = [];
  localStorage.removeItem(STORAGE_KEY);
}

/**
 * Remove a specific entry
 */
export function removeFromHistory(id: string): void {
  const history = getHistory().filter(e => e.id !== id);
  if (shouldStoreHistory()) {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(history));
  } else {
    inMemoryHistory = history;
  }
}

/**
 * Mark entry as favorite (moves to separate storage)
 */
export function toggleFavorite(id: string): boolean {
  if (!shouldStoreHistory()) {
    return false;
  }
  const favorites = getFavorites();
  const isFavorite = favorites.some(f => f.id === id);

  if (isFavorite) {
    // Remove from favorites
    const newFavorites = favorites.filter(f => f.id !== id);
    localStorage.setItem('qoredb_favorites', JSON.stringify(newFavorites));
    return false;
  } else {
    // Add to favorites
    const entry = getHistory().find(e => e.id === id);
    if (entry) {
      favorites.unshift({
        ...entry,
        query: redactQuery(entry.query),
      });
      localStorage.setItem('qoredb_favorites', JSON.stringify(favorites));
    }
    return true;
  }
}

/**
 * Get favorite queries
 */
export function getFavorites(): HistoryEntry[] {
  if (!shouldStoreHistory()) {
    return [];
  }
  try {
    const data = localStorage.getItem('qoredb_favorites');
    if (!data) return [];
    return JSON.parse(data) as HistoryEntry[];
  } catch {
    return [];
  }
}

/**
 * Check if an entry is a favorite
 */
export function isFavorite(id: string): boolean {
  if (!shouldStoreHistory()) {
    return false;
  }
  return getFavorites().some(f => f.id === id);
}
