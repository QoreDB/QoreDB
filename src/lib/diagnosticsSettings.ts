export interface DiagnosticsSettings {
  storeHistory: boolean;
  storeErrorLogs: boolean;
}

const STORAGE_KEY = 'qoredb_diagnostics_settings';

const DEFAULT_SETTINGS: DiagnosticsSettings = {
  storeHistory: false,
  storeErrorLogs: false,
};

export function getDiagnosticsSettings(): DiagnosticsSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_SETTINGS;
    const parsed = JSON.parse(raw) as Partial<DiagnosticsSettings>;
    return {
      storeHistory: parsed.storeHistory ?? DEFAULT_SETTINGS.storeHistory,
      storeErrorLogs: parsed.storeErrorLogs ?? DEFAULT_SETTINGS.storeErrorLogs,
    };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

export function setDiagnosticsSettings(settings: DiagnosticsSettings): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
}

export function updateDiagnosticsSettings(
  patch: Partial<DiagnosticsSettings>
): DiagnosticsSettings {
  const next = { ...getDiagnosticsSettings(), ...patch };
  setDiagnosticsSettings(next);
  return next;
}

export function shouldStoreHistory(): boolean {
  return getDiagnosticsSettings().storeHistory;
}

export function shouldStoreErrorLogs(): boolean {
  return getDiagnosticsSettings().storeErrorLogs;
}
