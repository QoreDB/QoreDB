// SPDX-License-Identifier: BUSL-1.1

import { BookmarkCheck, Check, Circle, Loader2, Pencil, Play, Settings2, X } from 'lucide-react';
import { useCallback, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
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
import { confirmDialog } from '@/lib/stores/confirmStore';
import type { DiffSource } from '@/lib/tabs';
import { cn } from '@/lib/utils';
import { RecordingBanner } from './RecordingBanner';
import { RecordingDialog } from './RecordingDialog';
import { ReplayReportView } from './ReplayReportView';
import { ReplaySetPicker } from './ReplaySetPicker';

interface ReplayTabProps {
  sessionId: string | null;
  environment?: string;
  connectionName?: string;
  database?: string;
  onOpenDiff?: (left: DiffSource, right: DiffSource, title: string) => void;
}

export function ReplayTab({
  sessionId,
  environment,
  connectionName,
  database,
  onOpenDiff,
}: ReplayTabProps) {
  const { t } = useTranslation();
  const isProduction = environment === 'production';
  const [allowMutations, setAllowMutations] = useState(false);
  const [editingIgnored, setEditingIgnored] = useState<string | null>(null);
  const [baselineRunId, setBaselineRunId] = useState<string | undefined>();
  const [abSessionId, setAbSessionId] = useState<string | undefined>();
  const [setupOpen, setSetupOpen] = useState(false);

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
    dropMutations,
    acceptRun,
    runs,
    abReport,
    sessions,
    replay,
    replayAb,
    abortReplay,
    removeSet,
    updateIgnoredColumns,
  } = useReplay(sessionId);

  const otherSessions = sessions.filter(session => session.id !== sessionId);

  const activeReport: ReplayReport | null = abReport
    ? {
        run: abReport.right,
        baseline_run_id: abReport.left.run_id,
        results: abReport.results,
        summary: abReport.summary,
      }
    : report;

  const mutationCount = useMemo(
    () => activeSet?.entries.filter(entry => entry.is_mutation).length ?? 0,
    [activeSet]
  );

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

  const handleDeleteSet = useCallback(
    async (slug: string) => {
      const set = sets.find(candidate => candidate.slug === slug);
      const ok = await confirmDialog({
        title: t('replay.confirm.deleteTitle', { name: set?.name ?? slug }),
        description: t('replay.confirm.deleteBody'),
      });
      if (ok) await removeSet(slug);
    },
    [removeSet, sets, t]
  );

  const handleAccept = useCallback(
    async (entryIds?: string[]) => {
      // A/B compares two live runs; neither is the set's reference.
      if (!report) return;
      const ok = await confirmDialog({
        title: entryIds
          ? t('replay.confirm.acceptEntryTitle')
          : t('replay.confirm.acceptAllTitle', { count: report.summary.total }),
        description: t('replay.confirm.acceptBody'),
      });
      if (ok) await acceptRun(report.run.run_id, entryIds);
    },
    [acceptRun, report, t]
  );

  const target = [connectionName, database, environment].filter(Boolean).join(' · ');

  return (
    <div className="flex-1 flex flex-col min-h-0">
      {recording && (
        <RecordingBanner
          recording={recording}
          previews={previews}
          onStop={() => void endRecording()}
          onCancel={() => void abortRecording()}
          onDiscard={index => void dropRecorded(index)}
          onDiscardMutations={() => void dropMutations()}
        />
      )}

      <div className="shrink-0 flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-2 border-b border-border bg-muted/10">
        <div className="min-w-0 flex-1">
          <ReplaySetPicker
            sets={sets}
            activeSlug={activeSlug}
            activeName={activeSet?.name}
            loading={setsLoading}
            onSelect={slug => {
              setBaselineRunId(undefined);
              setAbSessionId(undefined);
              setEditingIgnored(null);
              void selectSet(slug);
            }}
            onDelete={slug => void handleDeleteSet(slug)}
          />
          {activeSet &&
            (editingIgnored === null ? (
              <p className="flex items-center gap-1.5 pl-2 text-[11px] text-muted-foreground">
                <span className="truncate">
                  {t('replay.recordedOn', {
                    driver: activeSet.source.driver_id,
                    environment: activeSet.source.environment,
                  })}
                  {' · '}
                  {activeSet.ignored_columns.length > 0
                    ? t('replay.ignoring', { columns: activeSet.ignored_columns.join(', ') })
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
                className="flex items-center gap-1.5 pl-2"
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
            ))}
        </div>

        <div className="flex items-center gap-2 shrink-0">
          {target && (
            <span
              className={cn(
                'truncate rounded-md border border-border px-2 py-1 text-[11px] text-muted-foreground',
                isProduction &&
                  'border-[var(--color-error)]/40 bg-[var(--color-error)]/5 text-[var(--color-error)]'
              )}
              title={t('replay.replayTarget', { target })}
            >
              {t('replay.replayTarget', { target })}
            </span>
          )}

          {activeSet && (runs.length > 1 || otherSessions.length > 0) && (
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
              <SelectTrigger className="h-7 w-52 text-xs">
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

          {activeSet && (
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 w-7 p-0"
                  aria-label={t('replay.options')}
                  title={t('replay.options')}
                >
                  <Settings2 size={14} />
                </Button>
              </PopoverTrigger>
              <PopoverContent align="end" className="w-72 space-y-3 p-3">
                {mutationCount > 0 && (
                  <label
                    htmlFor="replay-allow-mutations"
                    className="flex items-center justify-between gap-3 text-xs"
                  >
                    {t('replay.includeMutations', { count: mutationCount })}
                    <Switch
                      id="replay-allow-mutations"
                      checked={allowMutations}
                      onCheckedChange={setAllowMutations}
                      disabled={isProduction}
                    />
                  </label>
                )}
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 w-full gap-1.5 text-xs"
                  disabled={!report}
                  onClick={() => void handleAccept()}
                >
                  <BookmarkCheck size={12} />
                  {t('replay.acceptAll')}
                </Button>
                <p className="text-[11px] text-muted-foreground">{t('replay.acceptAllHint')}</p>
              </PopoverContent>
            </Popover>
          )}

          {activeSet &&
            (running ? (
              <Button variant="outline" size="sm" className="h-7 gap-1.5" onClick={abortReplay}>
                <X size={14} />
                {t('common.cancel')}
              </Button>
            ) : (
              <Button
                size="sm"
                className="h-7 gap-1.5"
                disabled={!sessionId || activeSet.redacted}
                title={activeSet.redacted ? t('replay.redactedCannotReplay') : undefined}
                onClick={() => {
                  const options = { ...DEFAULT_RUN_OPTIONS, allow_mutations: allowMutations };
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
            ))}

          {!recording && (
            <Button
              variant="outline"
              size="sm"
              className="h-7 gap-1.5"
              disabled={!sessionId}
              onClick={() => setSetupOpen(true)}
            >
              <Circle size={9} className="fill-current" />
              {t('replay.newRecording')}
            </Button>
          )}
        </div>
      </div>

      <div className="flex-1 min-h-0 p-4">
        {!activeSet ? (
          <div className="flex h-full items-center justify-center px-6">
            <p className="max-w-md text-center text-sm text-muted-foreground">
              {sets.length === 0 ? t('replay.emptyState') : t('replay.selectSetHint')}
            </p>
          </div>
        ) : activeSet.redacted ? (
          <p className="py-8 text-center text-xs text-muted-foreground">
            {t('replay.redactedCannotReplay')}
          </p>
        ) : running ? (
          <div className="flex h-full flex-col items-center justify-center gap-2">
            <Loader2 size={20} className="animate-spin text-muted-foreground" />
            {progress && (
              <>
                <p className="text-xs text-muted-foreground">
                  {t('replay.progress', { completed: progress.completed, total: progress.total })}
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
            onAcceptEntry={report ? entry => void handleAccept([entry.entry_id]) : undefined}
            leftLabel={abReport ? t('replay.sideA') : undefined}
            rightLabel={abReport ? t('replay.sideB') : undefined}
          />
        ) : (
          <p className="text-center text-xs text-muted-foreground py-8">{t('replay.notRunYet')}</p>
        )}
      </div>

      <RecordingDialog
        open={setupOpen}
        onOpenChange={setSetupOpen}
        isProduction={isProduction}
        onStart={setup => void beginRecording(setup)}
      />
    </div>
  );
}
