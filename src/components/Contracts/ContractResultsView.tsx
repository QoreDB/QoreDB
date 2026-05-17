// SPDX-License-Identifier: BUSL-1.1

import { AlertCircle, CheckCircle2, ChevronDown, ChevronRight, MinusCircle, XCircle } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import type { ContractRun, RuleResult, RuleStatus, RuleType } from '@/lib/contracts';

interface Props {
  run: ContractRun;
}

const STATUS_ICON: Record<RuleStatus, typeof CheckCircle2> = {
  pass: CheckCircle2,
  fail: XCircle,
  skipped: MinusCircle,
  error: AlertCircle,
};

const STATUS_STYLES: Record<RuleStatus, string> = {
  pass: 'text-emerald-600 dark:text-emerald-400',
  fail: 'text-red-600 dark:text-red-400',
  skipped: 'text-muted-foreground',
  error: 'text-amber-600 dark:text-amber-400',
};

export function ContractResultsView({ run }: Props) {
  const { t } = useTranslation();
  const skipped = run.results.filter(r => r.status === 'skipped').length;

  return (
    <div className="flex flex-col gap-4">
      <SummaryRow
        pass={run.pass_count}
        fail={run.fail_count}
        error={run.error_count}
        skipped={skipped}
        durationMs={run.duration_ms}
      />

      <div className="rounded-md border border-border overflow-hidden">
        <table className="w-full text-sm">
          <thead className="bg-muted/40 text-xs uppercase tracking-wider text-muted-foreground">
            <tr>
              <th className="px-3 py-2 text-left font-medium w-8" />
              <th className="px-3 py-2 text-left font-medium">{t('contracts.results.rule')}</th>
              <th className="px-3 py-2 text-left font-medium">{t('contracts.results.type')}</th>
              <th className="px-3 py-2 text-left font-medium">{t('contracts.results.status')}</th>
              <th className="px-3 py-2 text-right font-medium">{t('contracts.results.violations')}</th>
              <th className="px-3 py-2 text-right font-medium">{t('contracts.results.metric')}</th>
              <th className="px-3 py-2 text-right font-medium">{t('contracts.results.duration')}</th>
            </tr>
          </thead>
          <tbody>
            {run.results.map(result => (
              <ResultRow key={result.id} result={result} />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

interface SummaryRowProps {
  pass: number;
  fail: number;
  error: number;
  skipped: number;
  durationMs: number;
}

function SummaryRow({ pass, fail, error, skipped, durationMs }: SummaryRowProps) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap gap-3 text-sm">
      <Stat
        label={t('contracts.status.pass')}
        value={pass}
        className="bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
      />
      <Stat
        label={t('contracts.status.fail')}
        value={fail}
        className="bg-red-500/10 text-red-700 dark:text-red-300"
      />
      <Stat
        label={t('contracts.status.error')}
        value={error}
        className="bg-amber-500/10 text-amber-700 dark:text-amber-300"
      />
      <Stat
        label={t('contracts.status.skipped')}
        value={skipped}
        className="bg-muted text-muted-foreground"
      />
      <Stat label={t('contracts.results.duration')} value={`${(durationMs / 1000).toFixed(2)}s`} />
    </div>
  );
}

function Stat({ label, value, className }: { label: string; value: number | string; className?: string }) {
  return (
    <div
      className={cn(
        'flex items-baseline gap-2 rounded-md border border-border/70 px-3 py-1.5',
        className
      )}
    >
      <span className="text-xs uppercase tracking-wider opacity-80">{label}</span>
      <span className="font-semibold tabular-nums">{value}</span>
    </div>
  );
}

function ResultRow({ result }: { result: RuleResult }) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const Icon = STATUS_ICON[result.status];
  const hasSamples = (result.samples?.length ?? 0) > 0;
  const canExpand = hasSamples || Boolean(result.error);

  return (
    <>
      <tr
        className={cn(
          'border-t border-border',
          canExpand && 'cursor-pointer hover:bg-muted/30'
        )}
        onClick={() => canExpand && setExpanded(v => !v)}
      >
        <td className="px-3 py-2 align-middle">
          {canExpand ? (
            expanded ? (
              <ChevronDown size={14} className="text-muted-foreground" />
            ) : (
              <ChevronRight size={14} className="text-muted-foreground" />
            )
          ) : null}
        </td>
        <td className="px-3 py-2 align-middle font-medium text-foreground">{result.id}</td>
        <td className="px-3 py-2 align-middle text-muted-foreground text-xs">
          {t(`contracts.ruleType.${result.rule_type as RuleType}`, { defaultValue: result.rule_type })}
        </td>
        <td className={cn('px-3 py-2 align-middle font-medium', STATUS_STYLES[result.status])}>
          <span className="inline-flex items-center gap-1.5">
            <Icon size={14} />
            {t(`contracts.status.${result.status}`)}
          </span>
        </td>
        <td className="px-3 py-2 align-middle text-right tabular-nums">
          {result.violations_count ?? '—'}
        </td>
        <td className="px-3 py-2 align-middle text-right tabular-nums">
          {result.metric != null ? formatMetric(result.metric) : '—'}
        </td>
        <td className="px-3 py-2 align-middle text-right tabular-nums text-muted-foreground">
          {result.duration_ms}ms
        </td>
      </tr>
      {expanded && (
        <tr className="border-t border-border bg-muted/20">
          <td colSpan={7} className="px-3 py-3">
            {result.error && (
              <div className="mb-2 text-xs text-amber-600 dark:text-amber-400 font-mono">
                {result.error}
              </div>
            )}
            {hasSamples && result.samples ? <SamplesTable samples={result.samples} /> : null}
            {!hasSamples && !result.error && (
              <p className="text-xs text-muted-foreground">{t('contracts.results.noSamples')}</p>
            )}
          </td>
        </tr>
      )}
    </>
  );
}

function SamplesTable({ samples }: { samples: Record<string, unknown>[] }) {
  const { t } = useTranslation();
  if (samples.length === 0) {
    return <p className="text-xs text-muted-foreground">{t('contracts.results.noSamples')}</p>;
  }
  const columnSet = new Set<string>();
  for (const row of samples) {
    for (const k of Object.keys(row)) {
      columnSet.add(k);
    }
  }
  const columns = Array.from(columnSet);

  return (
    <div className="overflow-x-auto rounded border border-border/70 bg-background">
      <table className="w-full text-xs">
        <thead className="bg-muted/30 text-muted-foreground">
          <tr>
            {columns.map(col => (
              <th key={col} className="px-2 py-1 text-left font-medium whitespace-nowrap">
                {col}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {samples.map((row, idx) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: violation samples have no stable id and never reorder
            <tr key={idx} className="border-t border-border/40">
              {columns.map(col => (
                <td key={col} className="px-2 py-1 font-mono whitespace-nowrap">
                  {renderCell(row[col])}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function renderCell(v: unknown): string {
  if (v === null || v === undefined) return 'NULL';
  if (typeof v === 'object') return JSON.stringify(v);
  return String(v);
}

function formatMetric(n: number): string {
  if (Number.isInteger(n)) return n.toLocaleString();
  return n.toFixed(2);
}

export function ToggleSamplesButton({ expanded, onToggle }: { expanded: boolean; onToggle: () => void }) {
  const { t } = useTranslation();
  return (
    <Button type="button" variant="ghost" size="sm" onClick={onToggle}>
      {expanded ? t('contracts.results.hideSamples') : t('contracts.results.showSamples')}
    </Button>
  );
}
