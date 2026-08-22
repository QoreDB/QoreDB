// SPDX-License-Identifier: BUSL-1.1

import { AlertTriangle, Check, GitCompare, MinusCircle, Rows3, Timer } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import type { ReplayEntryResult, ReplayReport, ReplayVerdict } from '@/lib/replay';
import { cn } from '@/lib/utils';

interface ReplayReportViewProps {
  report: ReplayReport;
  onOpenDiff?: (entry: ReplayEntryResult) => void;
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
  skipped: MinusCircle,
};

function verdictVariant(verdict: ReplayVerdict) {
  if (verdict === 'broken') return 'destructive' as const;
  if (verdict === 'match') return 'secondary' as const;
  if (verdict === 'skipped') return 'outline' as const;
  return 'default' as const;
}

function formatMs(ms: number): string {
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${ms.toFixed(1)} ms`;
}

export function ReplayReportView({
  report,
  onOpenDiff,
  leftLabel,
  rightLabel,
}: ReplayReportViewProps) {
  const { t } = useTranslation();
  const { summary } = report;
  const sides =
    leftLabel && rightLabel ? `${rightLabel} / ${leftLabel}` : t('replay.column.runVsExpected');

  const counters: Array<[ReplayVerdict, number]> = [
    ['broken', summary.broken],
    ['row_count_diff', summary.row_count_diff],
    ['digest_diff', summary.digest_diff],
    ['slower', summary.slower],
    ['skipped', summary.skipped],
    ['match', summary.matched],
  ];

  return (
    <div className="flex flex-col min-h-0 gap-3">
      <div className="flex flex-wrap items-center gap-2">
        {counters
          .filter(([, count]) => count > 0)
          .map(([verdict, count]) => {
            const Icon = VERDICT_ICON[verdict];
            return (
              <Badge key={verdict} variant={verdictVariant(verdict)}>
                <Icon size={11} />
                {count} {t(`replay.verdict.${verdict}`)}
              </Badge>
            );
          })}
        <span className="text-xs text-muted-foreground">
          {t('replay.entriesTotal', { count: summary.total })}
        </span>
      </div>

      {report.run.capture_stopped_reason && (
        <p className="text-xs text-muted-foreground">
          {t(`replay.captureStopped.${report.run.capture_stopped_reason}`)}
        </p>
      )}

      <div className="flex-1 min-h-0 overflow-y-auto">
        <table className="w-full text-xs">
          <thead className="sticky top-0 bg-[var(--color-bg-1)]">
            <tr className="text-left text-muted-foreground">
              <th className="w-8 px-2 py-1.5 font-medium">#</th>
              <th className="px-2 py-1.5 font-medium">{t('replay.column.query')}</th>
              <th className="w-28 px-2 py-1.5 font-medium">{t('replay.column.verdict')}</th>
              <th className="w-32 px-2 py-1.5 font-medium text-right">
                {t('replay.column.rows')}
                <span className="ml-1 font-normal text-[10px]">({sides})</span>
              </th>
              <th className="w-36 px-2 py-1.5 font-medium text-right">
                {t('replay.column.duration')}
              </th>
              <th className="w-10" />
            </tr>
          </thead>
          <tbody>
            {report.results.map(result => {
              const Icon = VERDICT_ICON[result.verdict];
              const rowsChanged = result.row_count !== result.expected_row_count;
              const canDiff = onOpenDiff && result.captured && result.verdict !== 'skipped';
              return (
                <tr
                  key={result.entry_id}
                  className={cn(
                    'border-t border-border align-top',
                    result.verdict === 'broken' && 'bg-[var(--color-error)]/5'
                  )}
                >
                  <td className="px-2 py-1.5 text-muted-foreground">{result.order}</td>
                  <td className="px-2 py-1.5">
                    <span className="font-mono break-all">{result.query_preview}</span>
                    {result.error && (
                      <p className="mt-0.5 text-[var(--color-error)]">{result.error}</p>
                    )}
                    {result.skip_reason && (
                      <p className="mt-0.5 text-muted-foreground">{result.skip_reason}</p>
                    )}
                    {result.partial_comparison && (
                      <p className="mt-0.5 text-muted-foreground">
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
                    <span className={cn(rowsChanged && 'text-[var(--color-warning)]')}>
                      {result.row_count ?? '—'}
                    </span>
                    <span className="text-muted-foreground">
                      {' '}
                      / {result.expected_row_count ?? '—'}
                    </span>
                  </td>
                  <td className="px-2 py-1.5 text-right tabular-nums">
                    <span
                      className={cn(result.verdict === 'slower' && 'text-[var(--color-warning)]')}
                    >
                      {formatMs(result.execution_time_ms)}
                    </span>
                    <span className="text-muted-foreground">
                      {' / '}
                      {formatMs(result.expected_execution_time_ms)}
                    </span>
                  </td>
                  <td className="px-2 py-1.5">
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
