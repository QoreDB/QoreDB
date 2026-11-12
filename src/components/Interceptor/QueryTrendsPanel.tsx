// SPDX-License-Identifier: Apache-2.0

import { ClipboardCopy, Lightbulb, RefreshCw, TrendingUp } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Line, LineChart } from 'recharts';
import { toast } from 'sonner';
import { Tooltip } from '@/components/ui/tooltip';
import { Driver } from '@/lib/connection/drivers';
import { suggestIndexesFromSqlQuery, supportsIndexSuggestions } from '@/lib/query/indexSuggestions';
import { useLicense } from '@/providers/LicenseProvider';
import {
  type FingerprintTrend,
  formatExecutionTime,
  getPerformanceClass,
  getPerformanceColor,
  getQueryTrends,
} from '../../lib/tauri/interceptor';
import { Button } from '../ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';

const DAY_OPTIONS = [7, 14, 30] as const;

export function QueryTrendsPanel() {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();
  const showRegressions = isFeatureEnabled('profiling');
  const showIndexSuggestions = isFeatureEnabled('index_suggestions');
  const [days, setDays] = useState<number>(14);
  const [trends, setTrends] = useState<FingerprintTrend[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setTrends(await getQueryTrends({ days }));
    } catch (err) {
      setError(err instanceof Error ? err.message : t('interceptor.trends.loadError'));
    } finally {
      setLoading(false);
    }
  }, [days, t]);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border">
        <p className="text-xs text-muted-foreground">{t('interceptor.trends.description')}</p>
        <div className="flex-1" />
        <div className="flex items-center gap-1">
          <Select value={String(days)} onValueChange={value => setDays(Number(value))}>
            <SelectTrigger size="sm" className="w-36 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {DAY_OPTIONS.map(option => (
                <SelectItem key={option} value={String(option)}>
                  {t('interceptor.trends.days', { count: option })}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button variant="ghost" size="icon" className="h-8 w-8" onClick={load} disabled={loading}>
            <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />
          </Button>
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-auto">
        {error ? (
          <p className="px-4 py-12 text-center text-error text-sm">{error}</p>
        ) : trends.length === 0 && !loading ? (
          <p className="px-4 py-12 text-center text-muted-foreground text-sm">
            {t('interceptor.trends.empty')}
          </p>
        ) : (
          <table className="w-full text-xs">
            <thead className="sticky top-0 bg-muted/40 text-[11px] uppercase tracking-wider text-muted-foreground">
              <tr>
                <th className="px-3 py-2 text-left font-medium">
                  {t('interceptor.trends.columns.query')}
                </th>
                <th className="px-2 py-2 text-right font-medium">
                  {t('interceptor.trends.columns.count')}
                </th>
                <th className="px-2 py-2 text-right font-medium">P50</th>
                <th className="px-2 py-2 text-right font-medium">P95</th>
                <th className="px-2 py-2 text-right font-medium">
                  {t('interceptor.trends.columns.errorRate')}
                </th>
                <th className="px-3 py-2 text-left font-medium">
                  {t('interceptor.trends.columns.sparkline')}
                </th>
              </tr>
            </thead>
            <tbody>
              {trends.map(trend => (
                <TrendRow
                  key={trend.fingerprint}
                  trend={trend}
                  showRegression={showRegressions}
                  showIndexSuggestions={showIndexSuggestions}
                />
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}

function TrendRow({
  trend,
  showRegression,
  showIndexSuggestions,
}: {
  trend: FingerprintTrend;
  showRegression: boolean;
  showIndexSuggestions: boolean;
}) {
  const { t } = useTranslation();
  const p95Color = getPerformanceColor(getPerformanceClass(trend.p95_ms));
  const errorTone = trend.error_rate > 0 ? 'var(--q-error)' : undefined;
  const regression = showRegression ? trend.regression : undefined;

  const indexSuggestion = useMemo(() => {
    if (!showIndexSuggestions || (trend.p95_ms < 50 && !trend.regression)) return null;
    const driver = (trend.driver_id as Driver) || Driver.Postgres;
    if (!supportsIndexSuggestions(driver)) return null;
    return suggestIndexesFromSqlQuery(trend.query_preview, driver)[0] ?? null;
  }, [showIndexSuggestions, trend.p95_ms, trend.regression, trend.driver_id, trend.query_preview]);

  return (
    <tr className="border-t border-border align-top hover:bg-muted/30">
      <td className="px-3 py-2 max-w-md">
        <div className="flex items-center gap-2">
          {regression && (
            <Tooltip
              content={t('interceptor.trends.regressionDetail', {
                recent: formatExecutionTime(regression.recent_p95_ms),
                baseline: formatExecutionTime(regression.baseline_p95_ms),
                count: regression.recent_count,
              })}
            >
              <span className="inline-flex items-center gap-1 rounded-sm bg-error/15 px-1.5 py-px text-[10px] font-semibold uppercase text-error">
                <TrendingUp size={10} />
                {t('interceptor.trends.regression')}
              </span>
            </Tooltip>
          )}
          <code className="font-mono truncate" title={trend.query_preview}>
            {trend.query_preview}
          </code>
        </div>
        <div className="flex items-center gap-2 text-[10px] text-muted-foreground mt-0.5">
          <span>
            {trend.driver_id}
            {trend.database ? ` · ${trend.database}` : ''}
          </span>
          {indexSuggestion && (
            <button
              type="button"
              onClick={() => {
                navigator.clipboard.writeText(indexSuggestion.sql);
                toast.success(t('common.copied'));
              }}
              title={indexSuggestion.sql}
              className="inline-flex items-center gap-1 rounded bg-amber-500/10 border border-amber-500/30 px-1.5 py-px text-[10px] font-mono text-amber-600 dark:text-amber-400 hover:bg-amber-500/20"
            >
              <Lightbulb size={10} />
              <span>{indexSuggestion.indexName}</span>
              <ClipboardCopy size={9} />
            </button>
          )}
        </div>
      </td>
      <td className="px-2 py-2 text-right tabular-nums">{trend.count.toLocaleString()}</td>
      <td className="px-2 py-2 text-right tabular-nums">{formatExecutionTime(trend.p50_ms)}</td>
      <td className="px-2 py-2 text-right tabular-nums font-medium" style={{ color: p95Color }}>
        {formatExecutionTime(trend.p95_ms)}
      </td>
      <td className="px-2 py-2 text-right tabular-nums" style={{ color: errorTone }}>
        {(trend.error_rate * 100).toFixed(1)}%
      </td>
      <td className="px-3 py-1">
        <LineChart width={96} height={24} data={trend.points}>
          <Line
            type="monotone"
            dataKey="p95_ms"
            dot={false}
            stroke="currentColor"
            strokeWidth={1.5}
            isAnimationActive={false}
          />
        </LineChart>
      </td>
    </tr>
  );
}
