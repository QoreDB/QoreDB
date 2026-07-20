// SPDX-License-Identifier: BUSL-1.1

import { createContext, type ReactNode, useCallback, useContext, useEffect, useState } from 'react';
import {
  AI_PROVIDERS,
  type AiConfig,
  type AiProvider,
  type AiProviderStatus,
  aiGetProviderStatus,
} from '@/lib/ai';

const STORAGE_KEY = 'qoredb_ai_provider';
const SAMPLE_ROWS_STORAGE_KEY = 'qoredb_ai_sample_rows';
const SENSITIVE_DATA_STORAGE_KEY = 'qoredb_ai_sensitive_data';
const MODELS_STORAGE_KEY = 'qoredb_ai_models';

export interface AiPreferencesContextValue {
  preferredProvider: AiProvider;
  setPreferredProvider: (p: AiProvider) => void;
  preferredModels: Partial<Record<AiProvider, string>>;
  setPreferredModel: (provider: AiProvider, model: string) => void;
  providerStatuses: AiProviderStatus[];
  isReady: boolean;
  refreshStatuses: () => Promise<void>;
  getConfig: () => AiConfig;
  includeSampleRows: boolean;
  setIncludeSampleRows: (enabled: boolean) => void;
  allowSensitiveData: boolean;
  setAllowSensitiveData: (enabled: boolean) => void;
}

const AiPreferencesContext = createContext<AiPreferencesContextValue | null>(null);

function loadSavedProvider(): AiProvider {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (AI_PROVIDERS.some(p => p.id === saved)) {
      return saved as AiProvider;
    }
  } catch {
    // ignore
  }
  return 'open_ai';
}

function loadSampleRowsPreference(): boolean {
  try {
    return localStorage.getItem(SAMPLE_ROWS_STORAGE_KEY) === 'true';
  } catch {
    return false;
  }
}

function loadSensitiveDataPreference(): boolean {
  try {
    return localStorage.getItem(SENSITIVE_DATA_STORAGE_KEY) === 'true';
  } catch {
    return false;
  }
}

function loadSavedModels(): Partial<Record<AiProvider, string>> {
  try {
    const saved = localStorage.getItem(MODELS_STORAGE_KEY);
    if (saved) return JSON.parse(saved) as Partial<Record<AiProvider, string>>;
  } catch {
    // ignore
  }
  return {};
}

export function AiPreferencesProvider({ children }: { children: ReactNode }) {
  const [preferredProvider, setPreferredProviderState] = useState<AiProvider>(loadSavedProvider);
  const [preferredModels, setPreferredModelsState] =
    useState<Partial<Record<AiProvider, string>>>(loadSavedModels);
  const [providerStatuses, setProviderStatuses] = useState<AiProviderStatus[]>([]);
  const [includeSampleRows, setIncludeSampleRowsState] =
    useState<boolean>(loadSampleRowsPreference);
  const [allowSensitiveData, setAllowSensitiveDataState] = useState<boolean>(
    loadSensitiveDataPreference
  );

  const refreshStatuses = useCallback(async () => {
    try {
      // Probe only the selected provider for users upgrading from the old
      // per-key scan. The backend remembers the result, so future launches do
      // not touch Keychain merely to render configuration badges.
      const statuses = await aiGetProviderStatus(preferredProvider);
      setProviderStatuses(statuses);
    } catch {
      // AI may not be available (Core build)
    }
  }, [preferredProvider]);

  useEffect(() => {
    refreshStatuses();
  }, [refreshStatuses]);

  const setPreferredProvider = useCallback((p: AiProvider) => {
    setPreferredProviderState(p);
    localStorage.setItem(STORAGE_KEY, p);
  }, []);

  const setPreferredModel = useCallback((provider: AiProvider, model: string) => {
    setPreferredModelsState(prev => {
      const next = { ...prev, [provider]: model };
      localStorage.setItem(MODELS_STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  const setIncludeSampleRows = useCallback((enabled: boolean) => {
    setIncludeSampleRowsState(enabled);
    localStorage.setItem(SAMPLE_ROWS_STORAGE_KEY, String(enabled));
  }, []);

  const setAllowSensitiveData = useCallback((enabled: boolean) => {
    setAllowSensitiveDataState(enabled);
    localStorage.setItem(SENSITIVE_DATA_STORAGE_KEY, String(enabled));
  }, []);

  const providerInfo = AI_PROVIDERS.find(p => p.id === preferredProvider);
  const isReady =
    (providerInfo && !providerInfo.requiresKey) ||
    (providerStatuses.find(s => s.provider === preferredProvider)?.has_key ?? false);

  const getConfig = useCallback(
    (): AiConfig => ({
      provider: preferredProvider,
      model: preferredModels[preferredProvider],
      allow_sensitive_data: allowSensitiveData,
    }),
    [preferredProvider, preferredModels, allowSensitiveData]
  );

  return (
    <AiPreferencesContext.Provider
      value={{
        preferredProvider,
        setPreferredProvider,
        preferredModels,
        setPreferredModel,
        providerStatuses,
        isReady,
        refreshStatuses,
        getConfig,
        includeSampleRows,
        setIncludeSampleRows,
        allowSensitiveData,
        setAllowSensitiveData,
      }}
    >
      {children}
    </AiPreferencesContext.Provider>
  );
}

export function useAiPreferences(): AiPreferencesContextValue {
  const ctx = useContext(AiPreferencesContext);
  if (!ctx) throw new Error('useAiPreferences must be used within AiPreferencesProvider');
  return ctx;
}
