// SPDX-License-Identifier: Apache-2.0

import { AlertCircle, CheckCircle2, Loader2, Puzzle, Trash2 } from 'lucide-react';
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { type PluginRun, usePluginOutput } from '@/providers/PluginOutputProvider';

/** Dedicated tab content that lists past plugin command runs and renders the
 *  selected run's payload. The list is the source of truth for selection;
 *  callers don't need to thread state through. */
export function PluginOutputView() {
  const { t } = useTranslation();
  const { runs, selectedRunId, selectRun, clear } = usePluginOutput();

  const selected = useMemo(
    () => runs.find(r => r.id === selectedRunId) ?? runs[0] ?? null,
    [runs, selectedRunId]
  );

  if (runs.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 text-muted-foreground">
        <Puzzle size={32} className="opacity-50" />
        <p className="text-sm">{t('pluginOutput.empty')}</p>
        <p className="text-xs">{t('pluginOutput.emptyHint')}</p>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0">
      <aside className="flex w-64 shrink-0 flex-col border-r border-border bg-muted/20">
        <header className="flex items-center justify-between border-b border-border px-3 py-2">
          <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            {t('pluginOutput.runsHeader')}
          </span>
          <Button
            variant="ghost"
            size="sm"
            onClick={clear}
            title={t('pluginOutput.clearAll')}
            aria-label={t('pluginOutput.clearAll')}
            className="h-6 px-1.5"
          >
            <Trash2 size={12} />
          </Button>
        </header>
        <ul className="flex-1 overflow-y-auto">
          {runs.map(run => (
            <li key={run.id}>
              <button
                type="button"
                onClick={() => selectRun(run.id)}
                aria-current={run.id === selected?.id ? 'true' : undefined}
                className={`flex w-full flex-col gap-0.5 border-l-2 px-3 py-2 text-left text-sm transition-colors ${
                  run.id === selected?.id
                    ? 'border-l-primary bg-accent'
                    : 'border-l-transparent hover:bg-accent/50'
                }`}
              >
                <div className="flex items-center gap-1.5">
                  <RunStatusIcon status={run.status} />
                  <span className="truncate font-medium">{run.commandLabel}</span>
                </div>
                <span className="truncate text-xs text-muted-foreground">{run.pluginName}</span>
                <span className="text-[10px] text-muted-foreground">
                  {formatTime(run.startedAt)}
                  {run.durationMs !== undefined ? ` · ${formatDuration(run.durationMs)}` : ''}
                </span>
              </button>
            </li>
          ))}
        </ul>
      </aside>
      <section className="flex min-w-0 flex-1 flex-col">
        {selected && <RunDetail run={selected} />}
      </section>
    </div>
  );
}

function RunStatusIcon({ status }: { status: PluginRun['status'] }) {
  if (status === 'loading') {
    return <Loader2 size={12} className="animate-spin text-muted-foreground" />;
  }
  if (status === 'error') {
    return <AlertCircle size={12} className="text-destructive" />;
  }
  return <CheckCircle2 size={12} className="text-green-600 dark:text-green-500" />;
}

function RunDetail({ run }: { run: PluginRun }) {
  const { t } = useTranslation();
  return (
    <>
      <header className="flex items-baseline justify-between gap-3 border-b border-border px-4 py-2">
        <div className="min-w-0">
          <h2 className="truncate text-sm font-semibold">
            {run.pluginName}: {run.commandLabel}
          </h2>
          <p className="text-xs text-muted-foreground">
            {formatTime(run.startedAt)}
            {run.durationMs !== undefined ? ` · ${formatDuration(run.durationMs)}` : ''}
          </p>
        </div>
      </header>
      <div className="min-h-0 flex-1 overflow-auto p-4">
        {run.status === 'loading' && (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 size={14} className="animate-spin" />
            {t('pluginOutput.runningCommand')}
          </div>
        )}
        {run.status === 'error' && (
          <div className="rounded border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
            <div className="mb-1 font-semibold">{t('pluginOutput.error')}</div>
            <pre className="whitespace-pre-wrap font-mono text-xs">{run.error}</pre>
          </div>
        )}
        {run.status === 'ok' && <ResultRenderer value={run.value} />}
      </div>
    </>
  );
}

/** Renders a plugin command result. Values are pretty-printed as JSON; strings
 *  pass through. Plugins return JSON-serialisable payloads via the runtime ABI,
 *  so a JSON tree (text form) covers the realistic cases. */
function ResultRenderer({ value }: { value: unknown }) {
  const { t } = useTranslation();
  if (value === undefined || value === null) {
    return <span className="text-sm text-muted-foreground">{t('pluginOutput.noResult')}</span>;
  }
  if (typeof value === 'string') {
    return <pre className="whitespace-pre-wrap font-mono text-xs">{value}</pre>;
  }
  let pretty: string;
  try {
    pretty = JSON.stringify(value, null, 2);
  } catch {
    pretty = String(value);
  }
  return <pre className="whitespace-pre-wrap font-mono text-xs">{pretty}</pre>;
}

function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString();
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}
