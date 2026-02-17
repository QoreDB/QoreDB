// SPDX-License-Identifier: Apache-2.0

export interface RecoveryTab {
  id: string;
  type: 'query' | 'table' | 'database' | 'diff';
  title: string;
  namespace?: {
    database: string;
    schema?: string;
  };
  tableName?: string;
}

export interface CrashRecoverySnapshot {
  updatedAt: number;
  projectId: string;
  connectionId: string;
  activeTabId: string | null;
  tabs: RecoveryTab[];
  queryDrafts: Record<string, string>;
  tableBrowserTabs: Record<string, string>;
  databaseBrowserTabs: Record<string, string>;
}

const STORAGE_KEY = 'qoredb_crash_recovery';
const LEGACY_KEYS = ['qoredb_crash_recovery_v2', 'qoredb_crash_recovery_v1'];

function normalizeSnapshot(raw: string): CrashRecoverySnapshot | null {
  try {
    const parsed = JSON.parse(raw) as Partial<CrashRecoverySnapshot>;
    if (!parsed.connectionId || !parsed.projectId) return null;

    return {
      updatedAt: typeof parsed.updatedAt === 'number' ? parsed.updatedAt : Date.now(),
      projectId: parsed.projectId,
      connectionId: parsed.connectionId,
      activeTabId: parsed.activeTabId ?? null,
      tabs: Array.isArray(parsed.tabs) ? parsed.tabs : [],
      queryDrafts: parsed.queryDrafts ?? {},
      tableBrowserTabs: parsed.tableBrowserTabs ?? {},
      databaseBrowserTabs: parsed.databaseBrowserTabs ?? {},
    };
  } catch {
    return null;
  }
}

export function getCrashRecoverySnapshot(): CrashRecoverySnapshot | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      return normalizeSnapshot(raw);
    }

    for (const key of LEGACY_KEYS) {
      const legacy = localStorage.getItem(key);
      if (!legacy) continue;
      const normalized = normalizeSnapshot(legacy);
      if (normalized) {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(normalized));
        localStorage.removeItem(key);
        return normalized;
      }
    }

    return null;
  } catch {
    return null;
  }
}

export function saveCrashRecoverySnapshot(snapshot: CrashRecoverySnapshot): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshot));
}

export function clearCrashRecoverySnapshot(): void {
  localStorage.removeItem(STORAGE_KEY);
  for (const key of LEGACY_KEYS) {
    localStorage.removeItem(key);
  }
}
