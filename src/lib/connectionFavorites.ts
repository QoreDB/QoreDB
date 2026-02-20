// SPDX-License-Identifier: Apache-2.0

const STORAGE_KEY = 'qoredb_favorite_connections';

function normalizeFavoriteIds(value: unknown): string[] {
  if (!Array.isArray(value)) return [];

  const normalized: string[] = [];
  const seen = new Set<string>();

  for (const entry of value) {
    if (typeof entry !== 'string') continue;
    const id = entry.trim();
    if (!id || seen.has(id)) continue;
    seen.add(id);
    normalized.push(id);
  }

  return normalized;
}

export function getFavoriteConnectionIds(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    return normalizeFavoriteIds(JSON.parse(raw));
  } catch {
    return [];
  }
}

export function saveFavoriteConnectionIds(connectionIds: string[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(normalizeFavoriteIds(connectionIds)));
  } catch {
    // Ignore localStorage write failures (quota/private mode)
  }
}

export function reconcileFavoriteConnectionIds(validConnectionIds: string[]): string[] {
  const validIds = new Set(validConnectionIds);
  const current = getFavoriteConnectionIds();
  const filtered = current.filter(connectionId => validIds.has(connectionId));

  if (filtered.length !== current.length) {
    saveFavoriteConnectionIds(filtered);
  }

  return filtered;
}
