/**
 * Profiling Panel
 *
 * UI component for viewing query performance metrics and slow queries.
 * All data is fetched from the backend (Rust) for security.
 */

import { useState, useCallback, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Activity,
  Clock,
  AlertTriangle,
  RefreshCw,
  Download,
  Trash2,
  Database,
  ChevronRight,
} from 'lucide-react';
import { Button } from '../ui/button';
import { ScrollArea } from '../ui/scroll-area';
import {
  getProfilingMetrics,
  getSlowQueries,
  resetProfilingMetrics,
  clearSlowQueries,
  exportProfilingData,
  formatExecutionTime,
  getPerformanceClass,
  getPerformanceColor,
  type ProfilingMetrics,
  type SlowQueryEntry,
  type Environment,
} from '../../lib/tauri/interceptor';

interface MetricCardProps {
  label: string;
  value: string | number;
  subLabel?: string;
  color?: string;
  icon?: React.ReactNode;
}

function MetricCard({ label, value, subLabel, color, icon }: MetricCardProps) {
  return (
    <div className="p-4 rounded-lg bg-muted/50 border border-border">
      <div className="flex items-center gap-2 text-muted-foreground mb-1">
        {icon}
        <span className="text-xs">{label}</span>
      </div>
      <p className={`text-2xl font-semibold ${color || ''}`}>{value}</p>
      {subLabel && <p className="text-xs text-muted-foreground mt-0.5">{subLabel}</p>}
    </div>
  );
}

interface PercentileBarProps {
  label: string;
  value: number;
  max: number;
}

function PercentileBar({ label, value, max }: PercentileBarProps) {
  const percentage = max > 0 ? Math.min((value / max) * 100, 100) : 0;
  const perfClass = getPerformanceClass(value);
  const color = getPerformanceColor(perfClass);

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-sm">
        <span className="text-muted-foreground">{label}</span>
        <span className="font-medium" style={{ color }}>
          {formatExecutionTime(value)}
        </span>
      </div>
      <div className="h-2 rounded-full bg-muted overflow-hidden">
        <div
          className="h-full rounded-full transition-all"
          style={{
            width: `${percentage}%`,
            backgroundColor: color,
          }}
        />
      </div>
    </div>
  );
}

interface OperationChartProps {
  data: Record<string, number>;
}

function OperationChart({ data }: OperationChartProps) {
  const { t } = useTranslation();
  const total = Object.values(data).reduce((a, b) => a + b, 0);

  const items = Object.entries(data)
    .filter(([, count]) => count > 0)
    .sort(([, a], [, b]) => b - a)
    .slice(0, 6);

  if (total === 0) {
    return (
      <p className="text-sm text-muted-foreground text-center py-4">
        {t('interceptor.profiling.noData')}
      </p>
    );
  }

  return (
    <div className="space-y-2">
      {items.map(([op, count]) => {
        const percentage = (count / total) * 100;
        return (
          <div key={op} className="space-y-1">
            <div className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground capitalize">{op}</span>
              <span className="font-medium">
                {count} ({percentage.toFixed(1)}%)
              </span>
            </div>
            <div className="h-1.5 rounded-full bg-muted overflow-hidden">
              <div
                className="h-full rounded-full bg-primary/60"
                style={{ width: `${percentage}%` }}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}

interface SlowQueryItemProps {
  query: SlowQueryEntry;
}

function SlowQueryItem({ query }: SlowQueryItemProps) {
  const { t } = useTranslation();
  const perfClass = getPerformanceClass(query.execution_time_ms);
  const color = getPerformanceColor(perfClass);

  const envColors: Record<Environment, string> = {
    development: 'bg-green-500/10 text-green-600',
    staging: 'bg-yellow-500/10 text-yellow-600',
    production: 'bg-red-500/10 text-red-600',
  };

  return (
    <div className="p-3 rounded-lg border border-border hover:bg-muted/50 transition-colors">
      <div className="flex items-start gap-3">
        <AlertTriangle className="w-4 h-4 mt-0.5 text-yellow-500 shrink-0" />
        <div className="flex-1 min-w-0 space-y-1">
          <div className="flex items-center gap-2">
            <span
              className={`text-xs px-1.5 py-0.5 rounded font-medium ${envColors[query.environment]}`}
            >
              {query.environment.toUpperCase()}
            </span>
            <span className="text-sm font-medium" style={{ color }}>
              {formatExecutionTime(query.execution_time_ms)}
            </span>
            {query.row_count != null && (
              <span className="text-xs text-muted-foreground">
                {query.row_count} {t('table.rows')}
              </span>
            )}
            <span className="text-xs text-muted-foreground">{query.driver_id}</span>
          </div>
          <p className="text-sm font-mono truncate">{query.query}</p>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <span>{new Date(query.timestamp).toLocaleString()}</span>
            {query.database && (
              <>
                <ChevronRight className="w-3 h-3" />
                <span>{query.database}</span>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

export function ProfilingPanel() {
  const { t } = useTranslation();
  const [metrics, setMetrics] = useState<ProfilingMetrics | null>(null);
  const [slowQueries, setSlowQueries] = useState<SlowQueryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<'overview' | 'slow'>('overview');

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
    if (window.confirm(t('interceptor.profiling.resetConfirm'))) {
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

  if (loading && !metrics) {
    return (
      <div className="flex items-center justify-center h-full">
        <RefreshCw className="w-5 h-5 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (error || !metrics) {
    return (
      <div className="p-4 text-center">
        <p className="text-destructive mb-2">{error || t('interceptor.profiling.loadError')}</p>
        <Button variant="outline" size="sm" onClick={loadData}>
          {t('common.retry')}
        </Button>
      </div>
    );
  }

  const executedQueries = metrics.successful_queries + metrics.failed_queries;
  const successRate =
    executedQueries > 0 ? ((metrics.successful_queries / executedQueries) * 100).toFixed(1) : '100';

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b border-border">
        <h2 className="text-lg font-semibold">{t('interceptor.profiling.title')}</h2>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={loadData} disabled={loading}>
            <RefreshCw className={`w-4 h-4 mr-1 ${loading ? 'animate-spin' : ''}`} />
            {t('interceptor.profiling.actions.refresh')}
          </Button>
          <Button variant="outline" size="sm" onClick={handleExport}>
            <Download className="w-4 h-4 mr-1" />
            {t('interceptor.profiling.actions.export')}
          </Button>
          <Button variant="outline" size="sm" onClick={handleReset}>
            <Trash2 className="w-4 h-4 mr-1" />
            {t('interceptor.profiling.actions.reset')}
          </Button>
        </div>
      </div>

      {/* Tabs */}
      <div className="flex gap-1 p-2 border-b border-border">
        <button
          type="button"
          className={`px-3 py-1.5 text-sm rounded transition-colors ${
            activeTab === 'overview' ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'
          }`}
          onClick={() => setActiveTab('overview')}
        >
          {t('interceptor.profiling.tabs.overview')}
        </button>
        <button
          type="button"
          className={`px-3 py-1.5 text-sm rounded transition-colors ${
            activeTab === 'slow' ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'
          }`}
          onClick={() => setActiveTab('slow')}
        >
          {t('interceptor.profiling.tabs.slowQueries', {
            count: slowQueries.length,
          })}
        </button>
      </div>

      {/* Content */}
      <ScrollArea className="flex-1">
        {activeTab === 'overview' ? (
          <div className="p-4 space-y-6">
            {/* Key Metrics */}
            <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
              <MetricCard
                label={t('interceptor.profiling.metrics.totalQueries')}
                value={metrics.total_queries.toLocaleString()}
                icon={<Activity className="w-4 h-4" />}
              />
              <MetricCard
                label={t('interceptor.profiling.metrics.successRate')}
                value={`${successRate}%`}
                color={parseFloat(successRate) >= 95 ? 'text-green-500' : 'text-yellow-500'}
                icon={<Database className="w-4 h-4" />}
              />
              <MetricCard
                label={t('interceptor.profiling.metrics.avgTime')}
                value={formatExecutionTime(metrics.avg_execution_time_ms)}
                icon={<Clock className="w-4 h-4" />}
              />
              <MetricCard
                label={t('interceptor.profiling.metrics.slowCount')}
                value={metrics.slow_query_count}
                color={metrics.slow_query_count > 0 ? 'text-yellow-500' : 'text-green-500'}
                icon={<AlertTriangle className="w-4 h-4" />}
              />
            </div>

            {/* Latency Percentiles */}
            <div className="space-y-3">
              <h3 className="text-sm font-medium">{t('interceptor.profiling.latency.title')}</h3>
              <div className="space-y-3">
                <PercentileBar
                  label={t('interceptor.profiling.latency.p50')}
                  value={metrics.p50_execution_time_ms}
                  max={metrics.p99_execution_time_ms || 1000}
                />
                <PercentileBar
                  label={t('interceptor.profiling.latency.p95')}
                  value={metrics.p95_execution_time_ms}
                  max={metrics.p99_execution_time_ms || 1000}
                />
                <PercentileBar
                  label={t('interceptor.profiling.latency.p99')}
                  value={metrics.p99_execution_time_ms}
                  max={metrics.p99_execution_time_ms || 1000}
                />
                <PercentileBar
                  label={t('interceptor.profiling.latency.max')}
                  value={metrics.max_execution_time_ms}
                  max={metrics.max_execution_time_ms || 1000}
                />
              </div>
            </div>

            {/* Operations Breakdown */}
            <div className="space-y-3">
              <h3 className="text-sm font-medium">{t('interceptor.profiling.operations.title')}</h3>
              <OperationChart data={metrics.by_operation_type} />
            </div>

            {/* Environment Breakdown */}
            <div className="space-y-3">
              <h3 className="text-sm font-medium">
                {t('interceptor.profiling.environments.title')}
              </h3>
              <div className="grid grid-cols-3 gap-2">
                {Object.entries(metrics.by_environment).map(([env, count]) => (
                  <div key={env} className="p-3 rounded-lg border border-border text-center">
                    <p className="text-lg font-semibold">{count}</p>
                    <p className="text-xs text-muted-foreground">{t(`environment.${env}`)}</p>
                  </div>
                ))}
              </div>
            </div>

            {/* Period Info */}
            <div className="text-xs text-muted-foreground text-center pt-4 border-t border-border">
              {t('interceptor.profiling.period', {
                date: new Date(metrics.period_start).toLocaleString(),
              })}
            </div>
          </div>
        ) : (
          <div className="p-4 space-y-2">
            {slowQueries.length === 0 ? (
              <p className="text-center text-muted-foreground py-8">
                {t('interceptor.profiling.noSlowQueries')}
              </p>
            ) : (
              slowQueries.map(query => <SlowQueryItem key={query.id} query={query} />)
            )}
          </div>
        )}
      </ScrollArea>
    </div>
  );
}
