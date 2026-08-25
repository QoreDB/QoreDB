// SPDX-License-Identifier: BUSL-1.1

import {
  AlertTriangle,
  BookmarkCheck,
  Check,
  ChevronDown,
  ChevronRight,
  GitCompare,
  HelpCircle,
  MinusCircle,
  Rows3,
  Timer,
} from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  comparedCount,
  hasResultDiff,
  type ReplayEntryResult,
  type ReplayReport,
  type ReplayVerdict,
} from '@/lib/replay';
import { cn } from '@/lib/utils';

interface ReplayReportViewProps {
  report: ReplayReport;
  onOpenDiff?: (entry: ReplayEntryResult) => void;
  /** Promotes this run's result to the set's reference, for one entry. */
  onAcceptEntry?: (entry: ReplayEntryResult) => void;
  /** Names of the two compared sides. Defaults to run vs. recording. */
  leftLabel?: string;
  rightLabel?: string;
}

const VERDICT_ICON: Record<ReplayVerdict, typeof Check> = {
  match: Check,
  broken: AlertTriangle,
  row_count_diff: Rows3,
  digest_diff: GitCompare,
  slower: Timer,
  not_compared: HelpCircle,
  skipped: MinusCircle,
};

function verdictVariant(verdict: ReplayVerdict) {
  if (verdict === 'broken') return 'destructive' as const;
  if (verdict === 'match') return 'secondary' as const;
  if (verdict === 'skipped' || verdict === 'not_compared') return 'outline' as const;
  return 'default' as const;
}

function formatMs(ms: number): string {
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${ms.toFixed(1)} ms`;
}

const VERDICT_ORDER: ReplayVerdict[] = [
  'broken',
  'row_count_diff',
  'digest_diff',
  'slower',
  'not_compared',
  'skipped',
  'match',
];

export function ReplayReportView({
  report,
  onOpenDiff,
  onAcceptEntry,
  leftLabel,
  rightLabel,
}: ReplayReportViewProps) {
  const { t } = useTranslation();
  const { summary } = report;
  const [filter, setFilter] = useState<ReplayVerdict | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const toggle = (id: string) =>
    setExpanded(current => {
      const next = new Set(current);
      if (!next.delete(id)) next.add(id);
      return next;
    });

  const counts: Record<ReplayVerdict, number> = {
    broken: summary.broken,
    row_count_diff: summary.row_count_diff,
    digest_diff: summary.digest_diff,
    slower: summary.slower,
    not_compared: summary.not_compared,
    skipped: summary.skipped,
    match: summary.matched,
  };

  const coverage = [
    t('replay.coverage.compared', { compared: comparedCount(summary), total: summary.total }),
    summary.not_compared > 0 && t('replay.coverage.notCompared', { count: summary.not_compared }),
    summary.skipped > 0 && t('replay.coverage.skipped', { count: summary.skipped }),
    summary.broken > 0 && t('replay.coverage.broken', { count: summary.broken }),
  ]
    .filter(Boolean)
    .join(' · ');

  const results = filter ? report.results.filter(r => r.verdict === filter) : report.results;
  const sides =
    leftLabel && rightLabel ? `${leftLabel} → ${rightLabel}` : t('replay.column.referenceToRun');

  return (
    <div className="flex flex-col min-h-0 gap-2">
      <div className="flex flex-wrap items-center gap-2">
        {VERDICT_ORDER.filter(verdict => counts[verdict] > 0).map(verdict => {
          const Icon = VERDICT_ICON[verdict];
          const active = filter === verdict;
          return (
            <button
              key={verdict}
              type="button"
              onClick={() => setFilter(active ? null : verdict)}
              aria-pressed={active}
              className="rounded-full focus:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            >
              <Badge
                variant={verdictVariant(verdict)}
                className={cn('cursor-pointer', active && 'ring-1 ring-ring')}
              >
                <Icon size={11} />
                {counts[verdict]} {t(`replay.verdict.${verdict}`)}
              </Badge>
            </button>
          );
        })}
        {filter && (
          <Button
            variant="ghost"
            size="sm"
            className="h-5 px-1.5 text-xs"
            onClick={() => setFilter(null)}
          >
            {t('replay.clearFilter')}
          </Button>
        )}
      </div>

      <p className="text-xs text-muted-foreground">
        {coverage}
        {report.run.capture_stopped_reason && (
          <> · {t(`replay.captureStopped.${report.run.capture_stopped_reason}`)}</>
        )}
      </p>

      <div className="flex-1 min-h-0 overflow-y-auto">
        <table className="w-full table-fixed text-xs">
          <thead className="sticky top-0 bg-[var(--color-bg-1)]">
            <tr className="text-left text-muted-foreground">
              <th className="w-8 px-2 py-1.5 font-medium">#</th>
              <th className="px-2 py-1.5 font-medium">{t('replay.column.query')}</th>
              <th className="w-28 px-2 py-1.5 font-medium">{t('replay.column.verdict')}</th>
              <th className="w-32 px-2 py-1.5 font-medium text-right">
                {t('replay.column.rows')}
                <span className="ml-1 font-normal text-[10px]">({sides})</span>
              </th>
              <th className="w-40 px-2 py-1.5 font-medium text-right">
                {t('replay.column.duration')}
              </th>
              <th className="w-16" />
            </tr>
          </thead>
          <tbody>
            {results.map(result => {
              const Icon = VERDICT_ICON[result.verdict];
              const isOpen = expanded.has(result.entry_id);
              const canDiff = onOpenDiff && result.captured && hasResultDiff(result.verdict);
              const canAccept =
                onAcceptEntry && result.verdict !== 'skipped' && result.verdict !== 'match';
              const skipReason = result.skip_code
                ? t(`replay.skipReason.${result.skip_code}`)
                : result.skip_reason;
              return (
                <tr
                  key={result.entry_id}
                  className={cn(
                    'group border-t border-border align-top',
                    result.verdict === 'broken' && 'bg-[var(--color-error)]/5'
                  )}
                >
                  <td className="px-2 py-1.5 text-muted-foreground">{result.order}</td>
                  <td className="px-2 py-1.5">
                    <div className="flex items-start gap-1">
                      <button
                        type="button"
                        onClick={() => toggle(result.entry_id)}
                        aria-expanded={isOpen}
                        aria-label={isOpen ? t('replay.collapseQuery') : t('replay.expandQuery')}
                        className="mt-0.5 shrink-0 text-muted-foreground hover:text-foreground focus:outline-none focus-visible:ring-1 focus-visible:ring-ring rounded-sm"
                      >
                        {isOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                      </button>
                      <span
                        className={cn(
                          'font-mono min-w-0',
                          isOpen ? 'whitespace-pre-wrap break-words' : 'block truncate'
                        )}
                      >
                        {result.query_preview}
                      </span>
                    </div>
                    {result.error && (
                      <p
                        className="mt-0.5 pl-4 text-[var(--color-error)] truncate"
                        title={result.error}
                      >
                        {result.error.split('\n')[0]}
                      </p>
                    )}
                    {isOpen && result.error?.includes('\n') && (
                      <pre className="mt-1 ml-4 max-h-32 overflow-auto rounded bg-[var(--color-bg-2)] p-2 text-[10px] whitespace-pre-wrap">
                        {result.error}
                      </pre>
                    )}
                    {skipReason && (
                      <p className="mt-0.5 pl-4 text-muted-foreground">{skipReason}</p>
                    )}
                    {result.partial_comparison && (
                      <p className="mt-0.5 pl-4 text-muted-foreground">
                        {t('replay.partialComparison')}
                      </p>
                    )}
                  </td>
                  <td className="px-2 py-1.5">
                    <Badge variant={verdictVariant(result.verdict)}>
                      <Icon size={11} />
                      {t(`replay.verdict.${result.verdict}`)}
                    </Badge>
                  </td>
                  <td className="px-2 py-1.5 text-right tabular-nums">
                    <span className="text-muted-foreground">
                      {result.expected_row_count ?? '—'}
                      {' → '}
                    </span>
                    <span
                      className={cn(
                        result.verdict === 'row_count_diff' && 'text-[var(--color-warning)]'
                      )}
                    >
                      {result.row_count ?? '—'}
                    </span>
                  </td>
                  <td className="px-2 py-1.5 text-right tabular-nums">
                    <span className="text-muted-foreground">
                      {formatMs(result.expected_execution_time_ms)}
                      {' → '}
                    </span>
                    <span
                      className={cn(result.verdict === 'slower' && 'text-[var(--color-warning)]')}
                    >
                      {result.verdict === 'skipped' ? '—' : formatMs(result.execution_time_ms)}
                    </span>
                  </td>
                  <td className="px-2 py-1.5">
                    <div className="flex items-center justify-end gap-0.5">
                      {canDiff && (
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-6 w-6 p-0"
                          onClick={() => onOpenDiff(result)}
                          aria-label={t('replay.openDiff')}
                          title={t('replay.openDiff')}
                        >
                          <GitCompare size={12} />
                        </Button>
                      )}
                      {canAccept && (
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-6 w-6 p-0 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100"
                          onClick={() => onAcceptEntry(result)}
                          aria-label={t('replay.acceptEntry')}
                          title={t('replay.acceptEntry')}
                        >
                          <BookmarkCheck size={12} />
                        </Button>
                      )}
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
