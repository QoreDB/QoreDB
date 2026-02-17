// SPDX-License-Identifier: BUSL-1.1

import { createContext, useContext, useState, useEffect, useCallback, type ReactNode } from 'react';
import {
  aiGetProviderStatus,
  type AiProvider,
  type AiConfig,
  type AiProviderStatus,
} from '@/lib/ai';

const STORAGE_KEY = 'qoredb_ai_provider';

export interface AiPreferencesContextValue {
  preferredProvider: AiProvider;
  setPreferredProvider: (p: AiProvider) => void;
  providerStatuses: AiProviderStatus[];
  isReady: boolean;
  refreshStatuses: () => Promise<void>;
  getConfig: () => AiConfig;
}

const AiPreferencesContext = createContext<AiPreferencesContextValue | null>(null);

function loadSavedProvider(): AiProvider {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === 'open_ai' || saved === 'anthropic' || saved === 'ollama') {
      return saved;
    }
  } catch {
    // ignore
  }
  return 'open_ai';
}

export function AiPreferencesProvider({ children }: { children: ReactNode }) {
  const [preferredProvider, setPreferredProviderState] = useState<AiProvider>(loadSavedProvider);
  const [providerStatuses, setProviderStatuses] = useState<AiProviderStatus[]>([]);

  const refreshStatuses = useCallback(async () => {
    try {
      const statuses = await aiGetProviderStatus();
      setProviderStatuses(statuses);
    } catch {
      // AI may not be available (Core build)
    }
  }, []);

  useEffect(() => {
    refreshStatuses();
  }, [refreshStatuses]);

  const setPreferredProvider = useCallback((p: AiProvider) => {
    setPreferredProviderState(p);
    localStorage.setItem(STORAGE_KEY, p);
  }, []);

  const isReady =
    preferredProvider === 'ollama' ||
    (providerStatuses.find(s => s.provider === preferredProvider)?.has_key ?? false);

  const getConfig = useCallback(
    (): AiConfig => ({ provider: preferredProvider }),
    [preferredProvider]
  );

  return (
    <AiPreferencesContext.Provider
      value={{
        preferredProvider,
        setPreferredProvider,
        providerStatuses,
        isReady,
        refreshStatuses,
        getConfig,
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
