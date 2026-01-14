/**
 * Query History Store
 * 
 * Persists query history to localStorage with session isolation.
 */

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

function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

/**
 * Get all history entries
 */
export function getHistory(): HistoryEntry[] {
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
  };
  
  // Add to beginning
  history.unshift(newEntry);
  
  // Trim to max entries
  if (history.length > MAX_ENTRIES) {
    history.splice(MAX_ENTRIES);
  }
  
  localStorage.setItem(STORAGE_KEY, JSON.stringify(history));
  
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
  return getHistory().filter(e => 
    e.query.toLowerCase().includes(lowerQuery)
  );
}

/**
 * Clear all history
 */
export function clearHistory(): void {
  localStorage.removeItem(STORAGE_KEY);
}

/**
 * Remove a specific entry
 */
export function removeFromHistory(id: string): void {
  const history = getHistory().filter(e => e.id !== id);
  localStorage.setItem(STORAGE_KEY, JSON.stringify(history));
}

/**
 * Mark entry as favorite (moves to separate storage)
 */
export function toggleFavorite(id: string): boolean {
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
      favorites.unshift(entry);
      localStorage.setItem('qoredb_favorites', JSON.stringify(favorites));
    }
    return true;
  }
}

/**
 * Get favorite queries
 */
export function getFavorites(): HistoryEntry[] {
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
  return getFavorites().some(f => f.id === id);
}
