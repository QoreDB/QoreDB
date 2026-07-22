// SPDX-License-Identifier: BUSL-1.1

import { createContext, type ReactNode, useCallback, useContext, useEffect, useState } from 'react';
import {
  AI_PROVIDERS,
  type AiConfig,
  type AiProvider,
  type AiProviderStatus,
  aiCheckProvider,
  aiGetLocalRuntimeStatus,
  aiGetProviderStatus,
  type LocalRuntimeStatus,
} from '@/lib/ai';

const STORAGE_KEY = 'qoredb_ai_provider';
const SAMPLE_ROWS_STORAGE_KEY = 'qoredb_ai_sample_rows';
const SENSITIVE_DATA_STORAGE_KEY = 'qoredb_ai_sensitive_data';
const MODELS_STORAGE_KEY = 'qoredb_ai_models';
const BASE_URLS_STORAGE_KEY = 'qoredb_ai_base_urls';

export interface AiPreferencesContextValue {
  preferredProvider: AiProvider;
  setPreferredProvider: (p: AiProvider) => void;
  preferredModels: Partial<Record<AiProvider, string>>;
  setPreferredModel: (provider: AiProvider, model: string) => void;
  preferredBaseUrls: Partial<Record<AiProvider, string>>;
  setPreferredBaseUrl: (provider: AiProvider, baseUrl: string) => void;
  providerStatuses: AiProviderStatus[];
  localRuntimeStatus: LocalRuntimeStatus | null;
  providerReady: Record<AiProvider, boolean>;
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

function loadSavedBaseUrls(): Partial<Record<AiProvider, string>> {
  try {
    const saved = localStorage.getItem(BASE_URLS_STORAGE_KEY);
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
  const [preferredBaseUrls, setPreferredBaseUrlsState] =
    useState<Partial<Record<AiProvider, string>>>(loadSavedBaseUrls);
  const [providerStatuses, setProviderStatuses] = useState<AiProviderStatus[]>([]);
  const [localRuntimeStatus, setLocalRuntimeStatus] = useState<LocalRuntimeStatus | null>(null);
  const [ollamaReady, setOllamaReady] = useState(false);
  const [includeSampleRows, setIncludeSampleRowsState] =
    useState<boolean>(loadSampleRowsPreference);
  const [allowSensitiveData, setAllowSensitiveDataState] = useState<boolean>(
    loadSensitiveDataPreference
  );

  const refreshStatuses = useCallback(async () => {
    const [providers, localRuntime, ollama] = await Promise.allSettled([
      // Probe only the selected provider for users upgrading from the old
      // per-key scan. The backend remembers the result, so future launches do
      // not touch Keychain merely to render configuration badges.
      aiGetProviderStatus(preferredProvider),
      aiGetLocalRuntimeStatus(),
      aiCheckProvider('ollama', preferredBaseUrls.ollama),
    ]);
    if (providers.status === 'fulfilled') setProviderStatuses(providers.value);
    if (localRuntime.status === 'fulfilled') setLocalRuntimeStatus(localRuntime.value);
    setOllamaReady(ollama.status === 'fulfilled' && ollama.value);
  }, [preferredProvider, preferredBaseUrls.ollama]);

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

  const setPreferredBaseUrl = useCallback((provider: AiProvider, baseUrl: string) => {
    setPreferredBaseUrlsState(prev => {
      const next = { ...prev, [provider]: baseUrl };
      if (!baseUrl) delete next[provider];
      localStorage.setItem(BASE_URLS_STORAGE_KEY, JSON.stringify(next));
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

  const providerReady = Object.fromEntries(
    AI_PROVIDERS.map(provider => {
      if (provider.kind === 'managed_local') {
        return [
          provider.id,
          localRuntimeStatus?.state === 'ready' || localRuntimeStatus?.state === 'running',
        ];
      }
      if (provider.kind === 'external_local') return [provider.id, ollamaReady];
      if (!provider.requiresKey) return [provider.id, true];
      return [
        provider.id,
        providerStatuses.find(status => status.provider === provider.id)?.has_key ?? false,
      ];
    })
  ) as Record<AiProvider, boolean>;
  const isReady = providerReady[preferredProvider];

  const getConfig = useCallback(
    (): AiConfig => ({
      provider: preferredProvider,
      model: preferredModels[preferredProvider],
      base_url: preferredBaseUrls[preferredProvider] || undefined,
      allow_sensitive_data: allowSensitiveData,
    }),
    [preferredProvider, preferredModels, preferredBaseUrls, allowSensitiveData]
  );

  return (
    <AiPreferencesContext.Provider
      value={{
        preferredProvider,
        setPreferredProvider,
        preferredModels,
        setPreferredModel,
        preferredBaseUrls,
        setPreferredBaseUrl,
        providerStatuses,
        localRuntimeStatus,
        providerReady,
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
