// SPDX-License-Identifier: Apache-2.0

import { listen } from '@/lib/transport';
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
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { useTheme } from '@/hooks/useTheme';
import {
  EMPTY_CONTRIBUTIONS,
  getPluginContributions,
  getPluginStatuses,
  type InstalledPlugin,
  listPlugins,
  type PluginContributions,
  type PluginLogEntry,
  type PluginLogEvent,
  type PluginNotifyEvent,
  type PluginRuntimeStatus,
} from '@/lib/plugins';

const ACTIVE_THEME_KEY = 'qoredb_plugin_theme';
/** Per-plugin in-memory log ring. Keeps the detail view bounded; older lines
 *  drop off the top. */
const MAX_LOG_ENTRIES = 200;

interface PluginContextValue {
  plugins: InstalledPlugin[];
  contributions: PluginContributions;
  /** Runtime status per executable plugin id (loaded, failures, grants). */
  statuses: Record<string, PluginRuntimeStatus>;
  /** Recent log lines per plugin id, oldest first. Capped per plugin. */
  logs: Record<string, PluginLogEntry[]>;
  /** Drops the accumulated log lines for one plugin. */
  clearLogs: (pluginId: string) => void;
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
  const { t } = useTranslation();
  const [plugins, setPlugins] = useState<InstalledPlugin[]>([]);
  const [contributions, setContributions] = useState<PluginContributions>(EMPTY_CONTRIBUTIONS);
  const [statuses, setStatuses] = useState<Record<string, PluginRuntimeStatus>>({});
  const [logs, setLogs] = useState<Record<string, PluginLogEntry[]>>({});
  const [loading, setLoading] = useState(true);
  const [activeThemeId, setActiveThemeIdState] = useState<string | null>(() =>
    localStorage.getItem(ACTIVE_THEME_KEY)
  );
  const injectedKeys = useRef<string[]>([]);
  // Monotonic id for log entries — a stable React key that timestamps can't
  // guarantee (two lines can land in the same millisecond).
  const logSeq = useRef(0);
  // The notify listener is mounted once; this ref keeps it reading the latest
  // plugin list when it needs to resolve an id to a display name.
  const pluginsRef = useRef<InstalledPlugin[]>([]);
  pluginsRef.current = plugins;

  const refresh = useCallback(async () => {
    try {
      const [list, contrib, statusList] = await Promise.all([
        listPlugins(),
        getPluginContributions(),
        getPluginStatuses(),
      ]);
      setPlugins(list);
      setContributions(contrib);
      setStatuses(Object.fromEntries(statusList.map(s => [s.pluginId, s])));
    } catch {
      // Plugins are non-critical: fall back to an empty state on error.
      setPlugins([]);
      setContributions(EMPTY_CONTRIBUTIONS);
      setStatuses({});
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Surface plugin-issued toasts. Backend emits `plugin-notify` whenever a
  // plugin granted the `notify` capability calls the matching host function.
  useEffect(() => {
    const unlistenPromise = listen<PluginNotifyEvent>('plugin-notify', evt => {
      const { level, message, code, pluginId } = evt.payload;
      const fn =
        level === 'success'
          ? toast.success
          : level === 'warning'
            ? toast.warning
            : level === 'error'
              ? toast.error
              : toast.info;

      // Host lifecycle notification: localize the headline from the code and
      // the plugin's display name, keeping the raw reason as the description.
      if (code === 'disabled') {
        const name =
          pluginsRef.current.find(p => p.manifest.id === pluginId)?.manifest.name ?? pluginId;
        fn(t('plugins.toast.disabledTitle', { name }), { description: message });
        // The instance was just unloaded — refresh so the badge updates.
        void refresh();
        return;
      }

      // Plugin-issued toast: attribute it to its plugin so the user always
      // knows which extension is talking.
      const name = pluginsRef.current.find(p => p.manifest.id === pluginId)?.manifest.name;
      fn(name ? `${name}: ${message}` : message);
    });
    return () => {
      void unlistenPromise.then(unlisten => unlisten());
    };
  }, [t, refresh]);

  // Accumulate plugin log lines (plugin `qoredb_log` calls + host lifecycle
  // events). Unlike toasts these persist so the detail view can show them.
  useEffect(() => {
    const unlistenPromise = listen<PluginLogEvent>('plugin-log', evt => {
      const { pluginId, level, message } = evt.payload;
      setLogs(prev => {
        logSeq.current += 1;
        const entry: PluginLogEntry = { id: logSeq.current, level, message, time: Date.now() };
        const next = [...(prev[pluginId] ?? []), entry];
        if (next.length > MAX_LOG_ENTRIES) next.splice(0, next.length - MAX_LOG_ENTRIES);
        return { ...prev, [pluginId]: next };
      });
    });
    return () => {
      void unlistenPromise.then(unlisten => unlisten());
    };
  }, []);

  const clearLogs = useCallback((pluginId: string) => {
    setLogs(prev => {
      if (!prev[pluginId]) return prev;
      const next = { ...prev };
      delete next[pluginId];
      return next;
    });
  }, []);

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
    () => ({
      plugins,
      contributions,
      statuses,
      logs,
      clearLogs,
      loading,
      activeThemeId,
      setActiveTheme,
      refresh,
    }),
    [
      plugins,
      contributions,
      statuses,
      logs,
      clearLogs,
      loading,
      activeThemeId,
      setActiveTheme,
      refresh,
    ]
  );

  return <PluginContext.Provider value={value}>{children}</PluginContext.Provider>;
}

export function usePlugins(): PluginContextValue {
  const ctx = useContext(PluginContext);
  if (!ctx) throw new Error('usePlugins must be used within PluginProvider');
  return ctx;
}
