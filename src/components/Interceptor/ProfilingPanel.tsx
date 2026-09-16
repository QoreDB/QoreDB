// SPDX-License-Identifier: Apache-2.0

// All data is fetched from the backend (Rust) for security.

import { Download, RefreshCw, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { EnvironmentChip } from '@/components/ui/environment-chip';
import { confirmDialog } from '@/lib/stores/confirmStore';
import {
  clearSlowQueries,
  exportProfilingData,
  formatExecutionTime,
  getPerformanceClass,
  getPerformanceColor,
  getProfilingMetrics,
  getSlowQueries,
  type ProfilingMetrics,
  resetProfilingMetrics,
  type SlowQueryEntry,
} from '../../lib/tauri/interceptor';
import { Button } from '../ui/button';

interface ProfilingPanelProps {
  view: 'overview' | 'slow';
}

function Stat({ label, value, tone }: { label: string; value: string; tone?: string }) {
  return (
    <div className="rounded-md border border-border px-3 py-2">
      <p className="text-[11px] uppercase tracking-wider text-muted-foreground">{label}</p>
      <p
        className="mt-0.5 text-lg font-semibold tabular-nums"
        style={tone ? { color: tone } : undefined}
      >
        {value}
      </p>
    </div>
  );
}

function PercentileBar({ label, value, max }: { label: string; value: number; max: number }) {
  const percentage = max > 0 ? Math.min((value / max) * 100, 100) : 0;
  const color = getPerformanceColor(getPerformanceClass(value));
  return (
    <div className="grid grid-cols-[6rem_1fr_5rem] items-center gap-3 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <div className="h-1.5 rounded-sm bg-muted overflow-hidden">
        <div
          className="h-full rounded-sm"
          style={{ width: `${percentage}%`, backgroundColor: color }}
        />
      </div>
      <span className="text-right font-medium tabular-nums" style={{ color }}>
        {formatExecutionTime(value)}
      </span>
    </div>
  );
}

function Breakdown({
  data,
  labelFor,
}: {
  data: Record<string, number>;
  labelFor: (key: string) => string;
}) {
  const { t } = useTranslation();
  const total = Object.values(data).reduce((a, b) => a + b, 0);
  const items = Object.entries(data)
    .filter(([, count]) => count > 0)
    .sort(([, a], [, b]) => b - a)
    .slice(0, 6);
  if (total === 0) {
    return <p className="text-xs text-muted-foreground">{t('interceptor.profiling.noData')}</p>;
  }
  return (
    <div className="space-y-1.5">
      {items.map(([key, count]) => {
        const percentage = (count / total) * 100;
        return (
          <div key={key} className="grid grid-cols-[6rem_1fr_6rem] items-center gap-3 text-xs">
            <span className="text-muted-foreground truncate">{labelFor(key)}</span>
            <div className="h-1.5 rounded-sm bg-muted overflow-hidden">
              <div className="h-full rounded-sm bg-accent/70" style={{ width: `${percentage}%` }} />
            </div>
            <span className="text-right tabular-nums">
              {count.toLocaleString()} · {percentage.toFixed(0)}%
            </span>
          </div>
        );
      })}
    </div>
  );
}

export function ProfilingPanel({ view }: ProfilingPanelProps) {
  const { t } = useTranslation();
  const [metrics, setMetrics] = useState<ProfilingMetrics | null>(null);
  const [slowQueries, setSlowQueries] = useState<SlowQueryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const [metricsData, slowData] = await Promise.all([getProfilingMetrics(), getSlowQueries()]);
      setMetrics(metricsData);
      setSlowQueries(slowData);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('interceptor.profiling.loadError'));
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleReset = useCallback(async () => {
    if (await confirmDialog({ description: t('interceptor.profiling.resetConfirm') })) {
      try {
        await Promise.all([resetProfilingMetrics(), clearSlowQueries()]);
        loadData();
      } catch (err) {
        console.error('Failed to reset profiling:', err);
      }
    }
  }, [loadData, t]);

  const handleExport = useCallback(async () => {
    try {
      const content = await exportProfilingData();
      const blob = new Blob([content], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `qoredb-profiling-${new Date().toISOString().split('T')[0]}.json`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      console.error('Failed to export profiling data:', err);
    }
  }, []);

  if (error || (!metrics && !loading)) {
    return (
      <div className="p-6 text-center">
        <p className="text-sm text-error mb-3">{error || t('interceptor.profiling.loadError')}</p>
        <Button variant="outline" size="sm" onClick={loadData}>
          {t('common.retry')}
        </Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-1 px-3 py-2 border-b border-border">
        {metrics && (
          <span className="text-xs text-muted-foreground">
            {t('interceptor.profiling.period', {
              date: new Date(metrics.period_start).toLocaleString(),
            })}
          </span>
        )}
        <div className="flex-1" />
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8"
          onClick={loadData}
          disabled={loading}
        >
          <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />
        </Button>
        <Button variant="ghost" size="sm" className="h-8" onClick={handleExport}>
          <Download size={14} className="mr-1.5" />
          {t('interceptor.profiling.actions.export')}
        </Button>
        <Button variant="ghost" size="sm" className="h-8" onClick={handleReset}>
          <Trash2 size={14} className="mr-1.5" />
          {t('interceptor.profiling.actions.reset')}
        </Button>
      </div>

      <div className="flex-1 min-h-0 overflow-auto">
        {metrics && view === 'overview' && <Overview metrics={metrics} />}
        {view === 'slow' && <SlowQueriesTable queries={slowQueries} loading={loading} />}
      </div>
    </div>
  );
}

function Overview({ metrics }: { metrics: ProfilingMetrics }) {
  const { t } = useTranslation();
  const executed = metrics.successful_queries + metrics.failed_queries;
  const successRate = executed > 0 ? (metrics.successful_queries / executed) * 100 : 100;
  const max = metrics.max_execution_time_ms || 1000;

  return (
    <div className="p-4 space-y-6">
      <div className="grid grid-cols-2 md:grid-cols-4 gap-2">
        <Stat
          label={t('interceptor.profiling.metrics.totalQueries')}
          value={metrics.total_queries.toLocaleString()}
        />
        <Stat
          label={t('interceptor.profiling.metrics.successRate')}
          value={`${successRate.toFixed(1)}%`}
          tone={successRate >= 95 ? 'var(--q-success)' : 'var(--q-warning)'}
        />
        <Stat
          label={t('interceptor.profiling.metrics.avgTime')}
          value={formatExecutionTime(metrics.avg_execution_time_ms)}
        />
        <Stat
          label={t('interceptor.profiling.metrics.slowCount')}
          value={metrics.slow_query_count.toLocaleString()}
          tone={metrics.slow_query_count > 0 ? 'var(--q-warning)' : undefined}
        />
      </div>

      <section className="space-y-2">
        <h3 className="text-[11px] uppercase tracking-wider text-muted-foreground">
          {t('interceptor.profiling.latency.title')}
        </h3>
        <PercentileBar
          label={t('interceptor.profiling.latency.p50')}
          value={metrics.p50_execution_time_ms}
          max={max}
        />
        <PercentileBar
          label={t('interceptor.profiling.latency.p95')}
          value={metrics.p95_execution_time_ms}
          max={max}
        />
        <PercentileBar
          label={t('interceptor.profiling.latency.p99')}
          value={metrics.p99_execution_time_ms}
          max={max}
        />
        <PercentileBar
          label={t('interceptor.profiling.latency.max')}
          value={metrics.max_execution_time_ms}
          max={max}
        />
      </section>

      <div className="grid md:grid-cols-2 gap-6">
        <section className="space-y-2">
          <h3 className="text-[11px] uppercase tracking-wider text-muted-foreground">
            {t('interceptor.profiling.operations.title')}
          </h3>
          <Breakdown data={metrics.by_operation_type} labelFor={key => key.toUpperCase()} />
        </section>
        <section className="space-y-2">
          <h3 className="text-[11px] uppercase tracking-wider text-muted-foreground">
            {t('interceptor.profiling.environments.title')}
          </h3>
          <Breakdown data={metrics.by_environment} labelFor={key => t(`environment.${key}`)} />
        </section>
      </div>
    </div>
  );
}

function SlowQueriesTable({ queries, loading }: { queries: SlowQueryEntry[]; loading: boolean }) {
  const { t } = useTranslation();
  if (queries.length === 0) {
    return (
      <p className="px-4 py-12 text-sm text-center text-muted-foreground">
        {loading ? '' : t('interceptor.profiling.noSlowQueries')}
      </p>
    );
  }
  return (
    <table className="w-full text-xs">
      <thead className="sticky top-0 bg-muted/40 text-[11px] uppercase tracking-wider text-muted-foreground">
        <tr>
          <th className="px-3 py-2 text-left font-medium">{t('interceptor.audit.columns.time')}</th>
          <th className="px-2 py-2 text-left font-medium">
            {t('interceptor.audit.columns.environment')}
          </th>
          <th className="px-2 py-2 text-left font-medium">
            {t('interceptor.audit.columns.query')}
          </th>
          <th className="px-2 py-2 text-left font-medium">
            {t('interceptor.audit.columns.database')}
          </th>
          <th className="px-2 py-2 text-right font-medium">
            {t('interceptor.audit.columns.duration')}
          </th>
          <th className="px-3 py-2 text-right font-medium">
            {t('interceptor.audit.columns.rows')}
          </th>
        </tr>
      </thead>
      <tbody>
        {queries.map(query => (
          <tr key={query.id} className="border-t border-border hover:bg-muted/30">
            <td className="px-3 py-1.5 whitespace-nowrap text-muted-foreground tabular-nums">
              {new Date(query.timestamp).toLocaleString()}
            </td>
            <td className="px-2 py-1.5">
              <EnvironmentChip environment={query.environment} />
            </td>
            <td className="px-2 py-1.5 max-w-0 w-full">
              <div className="font-mono truncate" title={query.query}>
                {query.query}
              </div>
              <div className="text-muted-foreground">{query.driver_id}</div>
            </td>
            <td className="px-2 py-1.5 whitespace-nowrap text-muted-foreground">
              {query.database ?? '—'}
            </td>
            <td
              className="px-2 py-1.5 text-right whitespace-nowrap font-medium tabular-nums"
              style={{ color: getPerformanceColor(getPerformanceClass(query.execution_time_ms)) }}
            >
              {formatExecutionTime(query.execution_time_ms)}
            </td>
            <td className="px-3 py-1.5 text-right whitespace-nowrap tabular-nums text-muted-foreground">
              {query.row_count ?? '—'}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
