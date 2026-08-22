// SPDX-License-Identifier: BUSL-1.1

import { Check, ListRestart, Loader2, Pencil, Play, Trash2, X } from 'lucide-react';
import { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { useReplay } from '@/hooks/useReplay';
import { notify } from '@/lib/notify';
import {
  DEFAULT_RUN_OPTIONS,
  loadReplayCapture,
  type ReplayEntryResult,
  type ReplayReport,
} from '@/lib/replay';
import type { DiffSource } from '@/lib/tabs';
import { cn } from '@/lib/utils';
import { RecordingPanel } from './RecordingPanel';
import { ReplayReportView } from './ReplayReportView';

interface ReplayTabProps {
  sessionId: string | null;
  environment?: string;
  onOpenDiff?: (left: DiffSource, right: DiffSource, title: string) => void;
}

export function ReplayTab({ sessionId, environment, onOpenDiff }: ReplayTabProps) {
  const { t } = useTranslation();
  const isProduction = environment === 'production';
  const [allowMutations, setAllowMutations] = useState(false);
  const [editingIgnored, setEditingIgnored] = useState<string | null>(null);
  const [baselineRunId, setBaselineRunId] = useState<string | undefined>();
  const [abSessionId, setAbSessionId] = useState<string | undefined>();

  const {
    sets,
    setsLoading,
    activeSet,
    activeSlug,
    recording,
    previews,
    report,
    progress,
    running,
    selectSet,
    beginRecording,
    endRecording,
    abortRecording,
    dropRecorded,
    runs,
    abReport,
    sessions,
    replay,
    replayAb,
    abortReplay,
    removeSet,
    updateIgnoredColumns,
  } = useReplay(sessionId);

  // Other live connections this set can be replayed against, side by side.
  const otherSessions = sessions.filter(session => session.id !== sessionId);

  const activeReport: ReplayReport | null = abReport
    ? {
        run: abReport.right,
        baseline_run_id: abReport.left.run_id,
        results: abReport.results,
        summary: abReport.summary,
      }
    : report;

  const handleOpenDiff = useCallback(
    async (entry: ReplayEntryResult) => {
      if (!onOpenDiff || !activeReport) return;
      const baselineRunId = activeReport.baseline_run_id;
      if (!baselineRunId) {
        notify.error(t('replay.errors.noBaseline'));
        return;
      }
      try {
        const [baseline, current] = await Promise.all([
          loadReplayCapture(baselineRunId, entry.entry_id),
          loadReplayCapture(activeReport.run.run_id, entry.entry_id),
        ]);
        onOpenDiff(
          {
            type: 'query',
            label: abReport ? t('replay.sideA') : t('replay.baseline'),
            result: baseline,
          },
          {
            type: 'query',
            label: abReport ? t('replay.sideB') : t('replay.currentRun'),
            result: current,
          },
          `${t('replay.title')}: ${entry.query_preview}`
        );
      } catch (err) {
        notify.error(t('replay.errors.loadCapture'), String(err));
      }
    },
    [abReport, activeReport, onOpenDiff, t]
  );

  return (
    <div className="flex-1 flex min-h-0">
      <aside className="w-72 shrink-0 border-r border-border flex flex-col min-h-0">
        <div className="shrink-0 flex items-center gap-2 px-3 py-3 border-b border-border bg-muted/10">
          <ListRestart size={16} className="text-accent" />
          <h2 className="text-sm font-semibold">{t('replay.title')}</h2>
        </div>

        <div className="p-3 border-b border-border">
          <RecordingPanel
            recording={recording}
            previews={previews}
            isProduction={isProduction}
            onStart={beginRecording}
            onStop={endRecording}
            onCancel={abortRecording}
            onDiscard={dropRecorded}
          />
        </div>

        <div className="flex-1 min-h-0 overflow-y-auto p-2">
          {setsLoading ? (
            <div className="flex items-center justify-center py-8">
              <Loader2 size={16} className="animate-spin text-muted-foreground" />
            </div>
          ) : sets.length === 0 ? (
            <p className="px-2 py-8 text-center text-xs text-muted-foreground">
              {t('replay.noSets')}
            </p>
          ) : (
            <ul className="space-y-0.5">
              {sets.map(set => (
                <li
                  key={set.slug}
                  className={cn(
                    'group flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-[var(--color-bg-2)]',
                    activeSlug === set.slug && 'bg-[var(--color-bg-2)]'
                  )}
                >
                  <button
                    type="button"
                    className="min-w-0 flex-1 text-left"
                    onClick={() => {
                      setBaselineRunId(undefined);
                      setAbSessionId(undefined);
                      setEditingIgnored(null);
                      void selectSet(set.slug);
                    }}
                  >
                    <p className="text-xs font-medium truncate">{set.name}</p>
                    <p className="text-[11px] text-muted-foreground truncate">
                      {t('replay.entriesTotal', { count: set.entry_count })} · {set.environment}
                    </p>
                  </button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-5 w-5 p-0 opacity-0 group-hover:opacity-100"
                    onClick={() => void removeSet(set.slug)}
                    aria-label={t('replay.deleteSet')}
                  >
                    <Trash2 size={12} />
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </aside>

      <section className="flex-1 min-w-0 flex flex-col min-h-0">
        {!activeSet ? (
          <div className="flex-1 flex items-center justify-center px-6">
            <p className="max-w-md text-center text-sm text-muted-foreground">
              {t('replay.emptyState')}
            </p>
          </div>
        ) : (
          <>
            <div className="shrink-0 flex items-center justify-between gap-3 px-4 py-3 border-b border-border bg-muted/10">
              <div className="min-w-0">
                <h3 className="text-sm font-medium truncate">{activeSet.name}</h3>
                {editingIgnored === null ? (
                  <p className="flex items-center gap-1.5 text-xs text-muted-foreground truncate">
                    <span className="truncate">
                      {t('replay.recordedOn', {
                        driver: activeSet.source.driver_id,
                        environment: activeSet.source.environment,
                      })}
                      {' · '}
                      {activeSet.ignored_columns.length > 0
                        ? t('replay.ignoring', {
                            columns: activeSet.ignored_columns.join(', '),
                          })
                        : t('replay.ignoringNone')}
                    </span>
                    <Button
                      variant="ghost"
                      size="sm"
                      className="h-4 w-4 shrink-0 p-0"
                      onClick={() => setEditingIgnored(activeSet.ignored_columns.join(', '))}
                      aria-label={t('replay.editIgnoredColumns')}
                      title={t('replay.editIgnoredColumns')}
                    >
                      <Pencil size={11} />
                    </Button>
                  </p>
                ) : (
                  <form
                    className="flex items-center gap-1.5"
                    onSubmit={event => {
                      event.preventDefault();
                      void updateIgnoredColumns(
                        editingIgnored
                          .split(',')
                          .map(column => column.trim())
                          .filter(Boolean)
                      );
                      setEditingIgnored(null);
                    }}
                  >
                    <Input
                      autoFocus
                      value={editingIgnored}
                      onChange={event => setEditingIgnored(event.target.value)}
                      onKeyDown={event => {
                        if (event.key === 'Escape') setEditingIgnored(null);
                      }}
                      placeholder="updated_at, last_seen_at"
                      aria-label={t('replay.ignoredColumns')}
                      className="h-6 w-64 text-xs font-mono"
                    />
                    <Button type="submit" variant="ghost" size="sm" className="h-6 w-6 p-0">
                      <Check size={12} />
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="h-6 w-6 p-0"
                      onClick={() => setEditingIgnored(null)}
                    >
                      <X size={12} />
                    </Button>
                  </form>
                )}
              </div>

              <div className="flex items-center gap-3 shrink-0">
                {(runs.length > 1 || otherSessions.length > 0) && (
                  <Select
                    value={abSessionId ? `conn:${abSessionId}` : (baselineRunId ?? 'auto')}
                    onValueChange={value => {
                      if (value.startsWith('conn:')) {
                        setAbSessionId(value.slice('conn:'.length));
                        setBaselineRunId(undefined);
                        return;
                      }
                      setAbSessionId(undefined);
                      setBaselineRunId(value === 'auto' ? undefined : value);
                    }}
                    disabled={running}
                  >
                    <SelectTrigger className="h-7 w-56 text-xs">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="auto">{t('replay.compareToBaseline')}</SelectItem>
                      {runs
                        .filter(run => !run.is_baseline)
                        .map(run => (
                          <SelectItem key={run.run_id} value={run.run_id}>
                            {t('replay.compareToRun', {
                              date: new Date(run.started_at).toLocaleString(),
                            })}
                          </SelectItem>
                        ))}
                      {otherSessions.map(session => (
                        <SelectItem key={session.id} value={`conn:${session.id}`}>
                          {t('replay.compareToConnection', { name: session.display_name })}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
                <label
                  htmlFor="replay-allow-mutations"
                  className="flex items-center gap-1.5 text-xs"
                >
                  <Switch
                    id="replay-allow-mutations"
                    checked={allowMutations}
                    onCheckedChange={setAllowMutations}
                    disabled={isProduction}
                  />
                  {t('replay.allowMutations')}
                </label>
                {running ? (
                  <Button variant="outline" size="sm" className="h-7 gap-1.5" onClick={abortReplay}>
                    <X size={14} />
                    {t('common.cancel')}
                  </Button>
                ) : (
                  <Button
                    size="sm"
                    className="h-7 gap-1.5"
                    disabled={!sessionId}
                    onClick={() => {
                      const options = {
                        ...DEFAULT_RUN_OPTIONS,
                        allow_mutations: allowMutations,
                      };
                      if (abSessionId) {
                        void replayAb(abSessionId, options);
                        return;
                      }
                      void replay(options, baselineRunId);
                    }}
                  >
                    <Play size={14} />
                    {t('replay.run')}
                  </Button>
                )}
              </div>
            </div>

            <div className="flex-1 min-h-0 p-4">
              {running ? (
                <div className="flex h-full flex-col items-center justify-center gap-2">
                  <Loader2 size={20} className="animate-spin text-muted-foreground" />
                  {progress && (
                    <>
                      <p className="text-xs text-muted-foreground">
                        {t('replay.progress', {
                          completed: progress.completed,
                          total: progress.total,
                        })}
                      </p>
                      <p className="max-w-md truncate font-mono text-[11px] text-muted-foreground">
                        {progress.current_query_preview}
                      </p>
                    </>
                  )}
                </div>
              ) : activeReport ? (
                <ReplayReportView
                  report={activeReport}
                  onOpenDiff={handleOpenDiff}
                  leftLabel={abReport ? t('replay.sideA') : undefined}
                  rightLabel={abReport ? t('replay.sideB') : undefined}
                />
              ) : (
                <p className="text-center text-xs text-muted-foreground py-8">
                  {t('replay.notRunYet')}
                </p>
              )}
            </div>
          </>
        )}
      </section>
    </div>
  );
}
