export interface RecoveryTab {
  id: string;
  type: 'query' | 'table' | 'database';
  title: string;
  namespace?: {
    database: string;
    schema?: string;
  };
  tableName?: string;
}

export interface CrashRecoverySnapshot {
  version: 1;
  updatedAt: number;
  projectId: string;
  connectionId: string;
  activeTabId: string | null;
  tabs: RecoveryTab[];
  queryDrafts: Record<string, string>;
}

const STORAGE_KEY = 'qoredb_crash_recovery_v1';

export function getCrashRecoverySnapshot(): CrashRecoverySnapshot | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as CrashRecoverySnapshot;
    if (parsed?.version !== 1) return null;
    if (!parsed.connectionId || !parsed.projectId) return null;
    return parsed;
  } catch {
    return null;
  }
}

export function saveCrashRecoverySnapshot(snapshot: CrashRecoverySnapshot): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshot));
}

export function clearCrashRecoverySnapshot(): void {
  localStorage.removeItem(STORAGE_KEY);
}
