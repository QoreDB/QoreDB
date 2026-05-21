// SPDX-License-Identifier: Apache-2.0

import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useTheme } from '@/hooks/useTheme';
import {
  EMPTY_CONTRIBUTIONS,
  getPluginContributions,
  type InstalledPlugin,
  listPlugins,
  type PluginContributions,
} from '@/lib/plugins';

const ACTIVE_THEME_KEY = 'qoredb_plugin_theme';

interface PluginContextValue {
  plugins: InstalledPlugin[];
  contributions: PluginContributions;
  loading: boolean;
  activeThemeId: string | null;
  setActiveTheme: (id: string | null) => void;
  refresh: () => Promise<void>;
}

const PluginContext = createContext<PluginContextValue | null>(null);

/**
 * Loads declarative plugins at startup, exposes their aggregated contributions,
 * and applies the selected plugin theme's design tokens to the document root.
 */
export function PluginProvider({ children }: { children: ReactNode }) {
  const { resolvedTheme } = useTheme();
  const [plugins, setPlugins] = useState<InstalledPlugin[]>([]);
  const [contributions, setContributions] = useState<PluginContributions>(EMPTY_CONTRIBUTIONS);
  const [loading, setLoading] = useState(true);
  const [activeThemeId, setActiveThemeIdState] = useState<string | null>(() =>
    localStorage.getItem(ACTIVE_THEME_KEY)
  );
  const injectedKeys = useRef<string[]>([]);

  const refresh = useCallback(async () => {
    try {
      const [list, contrib] = await Promise.all([listPlugins(), getPluginContributions()]);
      setPlugins(list);
      setContributions(contrib);
    } catch {
      // Plugins are non-critical: fall back to an empty state on error.
      setPlugins([]);
      setContributions(EMPTY_CONTRIBUTIONS);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const setActiveTheme = useCallback((id: string | null) => {
    setActiveThemeIdState(id);
    if (id) {
      localStorage.setItem(ACTIVE_THEME_KEY, id);
    } else {
      localStorage.removeItem(ACTIVE_THEME_KEY);
    }
  }, []);

  // Apply the active plugin theme's design tokens to `:root`, swapping the
  // light/dark variant whenever the resolved theme changes.
  useEffect(() => {
    const root = document.documentElement;
    for (const key of injectedKeys.current) {
      root.style.removeProperty(key);
    }
    injectedKeys.current = [];

    if (!activeThemeId) return;
    const theme = contributions.themes.find(t => t.id === activeThemeId);
    if (!theme) return;

    const vars = resolvedTheme === 'dark' ? theme.dark : theme.light;
    for (const [key, value] of Object.entries(vars)) {
      root.style.setProperty(key, value);
      injectedKeys.current.push(key);
    }
  }, [activeThemeId, contributions.themes, resolvedTheme]);

  const value = useMemo<PluginContextValue>(
    () => ({ plugins, contributions, loading, activeThemeId, setActiveTheme, refresh }),
    [plugins, contributions, loading, activeThemeId, setActiveTheme, refresh]
  );

  return <PluginContext.Provider value={value}>{children}</PluginContext.Provider>;
}

export function usePlugins(): PluginContextValue {
  const ctx = useContext(PluginContext);
  if (!ctx) throw new Error('usePlugins must be used within PluginProvider');
  return ctx;
}
