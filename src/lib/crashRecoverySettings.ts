// SPDX-License-Identifier: Apache-2.0

export interface CrashRecoverySettings {
  saveQueryDrafts: boolean;
  ttlHours: number;
}

const STORAGE_KEY = 'qoredb_crash_recovery_settings';

const DEFAULT_SETTINGS: CrashRecoverySettings = {
  saveQueryDrafts: true,
  ttlHours: 24,
};

export function getCrashRecoverySettings(): CrashRecoverySettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_SETTINGS;
    const parsed = JSON.parse(raw) as Partial<CrashRecoverySettings>;
    return {
      saveQueryDrafts: parsed.saveQueryDrafts ?? DEFAULT_SETTINGS.saveQueryDrafts,
      ttlHours: parsed.ttlHours ?? DEFAULT_SETTINGS.ttlHours,
    };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

export function setCrashRecoverySettings(settings: CrashRecoverySettings): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
}

export function updateCrashRecoverySettings(
  patch: Partial<CrashRecoverySettings>
): CrashRecoverySettings {
  const next = { ...getCrashRecoverySettings(), ...patch };
  setCrashRecoverySettings(next);
  return next;
}

export function shouldSaveQueryDrafts(): boolean {
  return getCrashRecoverySettings().saveQueryDrafts;
}
