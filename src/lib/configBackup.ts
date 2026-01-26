import type { DiagnosticsSettings } from './diagnosticsSettings';
import type { SafetyPolicy } from './tauri';
import { getDiagnosticsSettings } from './diagnosticsSettings';

export interface ConfigBackupV1 {
  type: 'qoredb_config_backup';
  version: 1;
  exportedAt: number;
  ui: {
    theme?: 'light' | 'dark' | 'auto';
    language?: string;
    diagnostics?: DiagnosticsSettings;
    onboardingCompleted?: boolean;
    analyticsEnabled?: boolean;
  };
  safetyPolicy?: SafetyPolicy;
}

const THEME_KEY = 'qoredb-theme';
const LANGUAGE_KEY = 'i18nextLng';
const ONBOARDING_KEY = 'qoredb_onboarding_completed';
const ANALYTICS_KEY = 'qoredb_analytics_enabled';
const DIAGNOSTICS_KEY = 'qoredb_diagnostics_settings';

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function readBool(key: string): boolean | undefined {
  const raw = localStorage.getItem(key);
  if (raw === 'true') return true;
  if (raw === 'false') return false;
  return undefined;
}

export function buildConfigBackupV1(input: { safetyPolicy?: SafetyPolicy }): ConfigBackupV1 {
  const themeRaw = localStorage.getItem(THEME_KEY);
  const theme =
    themeRaw === 'light' || themeRaw === 'dark' || themeRaw === 'auto' ? themeRaw : undefined;

  const languageRaw = localStorage.getItem(LANGUAGE_KEY);
  const language = typeof languageRaw === 'string' && languageRaw.trim() ? languageRaw : undefined;

  return {
    type: 'qoredb_config_backup',
    version: 1,
    exportedAt: Date.now(),
    ui: {
      theme,
      language,
      diagnostics: getDiagnosticsSettings(),
      onboardingCompleted: readBool(ONBOARDING_KEY),
      analyticsEnabled: readBool(ANALYTICS_KEY),
    },
    safetyPolicy: input.safetyPolicy,
  };
}

export function isConfigBackupV1(value: unknown): value is ConfigBackupV1 {
  if (!isRecord(value)) return false;
  if (value.type !== 'qoredb_config_backup') return false;
  if (value.version !== 1) return false;
  if (!isRecord(value.ui)) return false;
  return true;
}

export function applyConfigBackupV1(payload: ConfigBackupV1): {
  theme?: 'light' | 'dark' | 'auto';
  language?: string;
  diagnostics?: DiagnosticsSettings;
  onboardingCompleted?: boolean;
  analyticsEnabled?: boolean;
  safetyPolicy?: SafetyPolicy;
} {
  const ui = payload.ui;

  if (ui.theme) localStorage.setItem(THEME_KEY, ui.theme);
  if (ui.language) localStorage.setItem(LANGUAGE_KEY, ui.language);

  if (ui.diagnostics) {
    localStorage.setItem(DIAGNOSTICS_KEY, JSON.stringify(ui.diagnostics));
  }

  if (ui.onboardingCompleted !== undefined) {
    localStorage.setItem(ONBOARDING_KEY, String(ui.onboardingCompleted));
  }

  if (ui.analyticsEnabled !== undefined) {
    localStorage.setItem(ANALYTICS_KEY, String(ui.analyticsEnabled));
  }

  return {
    theme: ui.theme,
    language: ui.language,
    diagnostics: ui.diagnostics,
    onboardingCompleted: ui.onboardingCompleted,
    analyticsEnabled: ui.analyticsEnabled,
    safetyPolicy: payload.safetyPolicy,
  };
}

