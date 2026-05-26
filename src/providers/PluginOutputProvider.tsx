// SPDX-License-Identifier: Apache-2.0

import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useMemo,
  useState,
} from 'react';
import { runPluginCommand, splitContributionId } from '@/lib/plugins';
import { usePlugins } from './PluginProvider';

/** Ring buffer cap. Keeps the panel snappy even if a user fires many commands. */
const MAX_RUNS = 50;

export type PluginRunStatus = 'loading' | 'ok' | 'error';

export interface PluginRun {
  id: string;
  pluginId: string;
  commandId: string;
  pluginName: string;
  commandLabel: string;
  status: PluginRunStatus;
  value?: unknown;
  error?: string;
  startedAt: number;
  durationMs?: number;
}

interface PluginOutputContextValue {
  runs: PluginRun[];
  selectedRunId: string | null;
  selectRun: (id: string) => void;
  runCommand: (namespacedId: string) => Promise<void>;
  clear: () => void;
}

const PluginOutputContext = createContext<PluginOutputContextValue | null>(null);

/** Tracks the history of plugin command invocations so the dedicated tab can
 *  display them. Discovery is data-driven: the namespaced id is split here and
 *  the plugin's display name is resolved from the registry. */
export function PluginOutputProvider({ children }: { children: ReactNode }) {
  const { plugins, contributions } = usePlugins();
  const [runs, setRuns] = useState<PluginRun[]>([]);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);

  const selectRun = useCallback((id: string) => {
    setSelectedRunId(id);
  }, []);

  const clear = useCallback(() => {
    setRuns([]);
    setSelectedRunId(null);
  }, []);

  const runCommand = useCallback(
    async (namespacedId: string) => {
      const { pluginId } = splitContributionId(namespacedId);
      const pluginName =
        plugins.find(p => p.manifest.id === pluginId)?.manifest.name ?? pluginId;
      const commandLabel =
        contributions.commands.find(c => c.id === namespacedId)?.label ?? namespacedId;
      const { localId } = splitContributionId(namespacedId);

      const runId = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
      const startedAt = Date.now();
      const pending: PluginRun = {
        id: runId,
        pluginId,
        commandId: localId,
        pluginName,
        commandLabel,
        status: 'loading',
        startedAt,
      };
      setRuns(prev => [pending, ...prev].slice(0, MAX_RUNS));
      setSelectedRunId(runId);

      try {
        const value = await runPluginCommand(namespacedId);
        setRuns(prev =>
          prev.map(r =>
            r.id === runId
              ? { ...r, status: 'ok', value, durationMs: Date.now() - startedAt }
              : r
          )
        );
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setRuns(prev =>
          prev.map(r =>
            r.id === runId
              ? { ...r, status: 'error', error: message, durationMs: Date.now() - startedAt }
              : r
          )
        );
      }
    },
    [plugins, contributions.commands]
  );

  const value = useMemo<PluginOutputContextValue>(
    () => ({ runs, selectedRunId, selectRun, runCommand, clear }),
    [runs, selectedRunId, selectRun, runCommand, clear]
  );

  return <PluginOutputContext.Provider value={value}>{children}</PluginOutputContext.Provider>;
}

export function usePluginOutput(): PluginOutputContextValue {
  const ctx = useContext(PluginOutputContext);
  if (!ctx) throw new Error('usePluginOutput must be used within PluginOutputProvider');
  return ctx;
}
