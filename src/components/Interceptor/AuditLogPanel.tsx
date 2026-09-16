// SPDX-License-Identifier: Apache-2.0

// All data is fetched from the backend (Rust) for security.

import type { TFunction } from 'i18next';
import { ChevronLeft, ChevronRight, Hash, RefreshCw, Search, Trash2, X } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { LicenseBadge } from '@/components/License/LicenseBadge';
import { EnvironmentChip } from '@/components/ui/environment-chip';
import { confirmDialog } from '@/lib/stores/confirmStore';
import { cn } from '@/lib/utils';
import { useLicense } from '@/providers/LicenseProvider';
import {
  type AuditFilter,
  type AuditLogEntry,
  type AuditStats,
  BUILTIN_SAFETY_RULE_I18N,
  clearAuditLog,
  type Environment,
  formatExecutionTime,
  getAuditEntries,
  getAuditStats,
} from '../../lib/tauri/interceptor';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import { AuditEntryDetail } from './AuditEntryDetail';
import { AuditExportMenu } from './AuditExportMenu';
import { auditStatus } from './AuditStatusChip';

const PAGE_SIZE = 50;

type StatusFilter = 'all' | 'success' | 'failed' | 'blocked';

function formatTimestamp(timestamp: string, t: TFunction): string {
  const date = new Date(timestamp);
  const diff = Date.now() - date.getTime();
  if (diff < 60_000) return t('interceptor.audit.time.justNow');
  if (diff < 3_600_000) {
    return t('interceptor.audit.time.minutesAgo', { count: Math.floor(diff / 60_000) });
  }
  if (diff < 86_400_000) {
    return t('interceptor.audit.time.hoursAgo', { count: Math.floor(diff / 3_600_000) });
  }
  if (diff < 604_800_000) {
    return t('interceptor.audit.time.daysAgo', { count: Math.floor(diff / 86_400_000) });
  }
  return date.toLocaleDateString();
}

const STATUS_DOT = {
  success: 'bg-success',
  failed: 'bg-error',
  blocked: 'bg-warning',
} as const;

function StatCounter({
  label,
  value,
  active,
  onClick,
}: {
  label: string;
  value: number;
  active?: boolean;
  onClick?: () => void;
}) {
  const content = (
    <>
      <span className="text-[11px] uppercase tracking-wider text-muted-foreground">{label}</span>
      <span className="text-sm font-semibold tabular-nums">{value.toLocaleString()}</span>
    </>
  );
  if (!onClick) return <div className="flex items-baseline gap-1.5 px-2">{content}</div>;
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'flex items-baseline gap-1.5 rounded-md px-2 py-0.5 transition-colors hover:bg-muted',
        active && 'bg-accent-soft text-foreground'
      )}
    >
      {content}
    </button>
  );
}

export function AuditLogPanel() {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();
  const isAdvanced = isFeatureEnabled('audit_advanced');

  const getSafetyRuleLabel = useCallback(
    (ruleId?: string | null) => {
      if (!ruleId) return '';
      const keys = BUILTIN_SAFETY_RULE_I18N[ruleId];
      return keys ? t(keys.nameKey) : ruleId;
    },
    [t]
  );

  const [entries, setEntries] = useState<AuditLogEntry[]>([]);
  const [stats, setStats] = useState<AuditStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [page, setPage] = useState(0);
  const [selectedEntry, setSelectedEntry] = useState<AuditLogEntry | null>(null);
  const [search, setSearch] = useState('');
  const [debouncedSearch, setDebouncedSearch] = useState('');
  const [environmentFilter, setEnvironmentFilter] = useState<Environment | 'all'>('all');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');
  const [fingerprintFilter, setFingerprintFilter] = useState<string | null>(null);

  useEffect(() => {
    const timer = setTimeout(() => setDebouncedSearch(search), 300);
    return () => clearTimeout(timer);
  }, [search]);

  const loadData = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const filter: AuditFilter = { limit: PAGE_SIZE, offset: page * PAGE_SIZE };
      if (environmentFilter !== 'all') filter.environment = environmentFilter;
      if (debouncedSearch.trim()) filter.search = debouncedSearch.trim();
      if (statusFilter === 'success') filter.success = true;
      else if (statusFilter === 'failed') filter.success = false;
      else if (statusFilter === 'blocked') filter.blocked = true;
      if (fingerprintFilter) filter.fingerprint = fingerprintFilter;

      const [entriesData, statsData] = await Promise.all([
        getAuditEntries(filter),
        getAuditStats(),
      ]);
      setEntries(entriesData);
      setStats(statsData);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load audit log');
    } finally {
      setLoading(false);
    }
  }, [page, debouncedSearch, environmentFilter, statusFilter, fingerprintFilter]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const updateStatus = useCallback((value: StatusFilter) => {
    setStatusFilter(value);
    setPage(0);
  }, []);

  const toggleStatus = (value: StatusFilter) =>
    updateStatus(statusFilter === value ? 'all' : value);

  const handleClear = useCallback(async () => {
    if (await confirmDialog({ description: t('interceptor.audit.clearConfirm') })) {
      try {
        await clearAuditLog();
        loadData();
      } catch (err) {
        console.error('Failed to clear audit log:', err);
      }
    }
  }, [t, loadData]);

  const hasMore = entries.length === PAGE_SIZE;

  if (error) {
    return (
      <div className="p-6 text-center">
        <p className="text-sm text-error mb-3">{error}</p>
        <Button variant="outline" size="sm" onClick={loadData}>
          {t('common.retry')}
        </Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border">
        {isAdvanced ? (
          <>
            <div className="relative flex-1 max-w-xs">
              <Search
                size={14}
                className="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
              />
              <Input
                placeholder={t('search.placeholder')}
                value={search}
                onChange={e => {
                  setSearch(e.target.value);
                  setPage(0);
                }}
                className="h-8 pl-8 text-xs"
              />
            </div>
            <Select
              value={environmentFilter}
              onValueChange={v => {
                setEnvironmentFilter(v as Environment | 'all');
                setPage(0);
              }}
            >
              <SelectTrigger size="sm" className="w-36 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">
                  {t('interceptor.audit.filters.allEnvironments')}
                </SelectItem>
                <SelectItem value="development">{t('environment.development')}</SelectItem>
                <SelectItem value="staging">{t('environment.staging')}</SelectItem>
                <SelectItem value="production">{t('environment.production')}</SelectItem>
              </SelectContent>
            </Select>
            {fingerprintFilter && (
              <button
                type="button"
                onClick={() => {
                  setFingerprintFilter(null);
                  setPage(0);
                }}
                className="inline-flex items-center gap-1 h-8 rounded-md border border-border px-2 font-mono text-xs text-muted-foreground hover:text-foreground"
              >
                <Hash size={11} />
                {fingerprintFilter.slice(0, 12)}
                <X size={11} />
              </button>
            )}
          </>
        ) : (
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <LicenseBadge tier="pro" />
            {t('interceptor.audit.upgradeForFilters')}
          </div>
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
        {isAdvanced && (
          <>
            <AuditExportMenu />
            <Button variant="ghost" size="sm" className="h-8" onClick={handleClear}>
              <Trash2 size={14} className="mr-1.5" />
              {t('interceptor.audit.clearLog')}
            </Button>
          </>
        )}
      </div>

      {isAdvanced && stats && (
        <div className="flex items-center gap-1 px-2 py-1.5 border-b border-border overflow-x-auto">
          <StatCounter
            label={t('interceptor.audit.stats.total')}
            value={stats.total}
            active={statusFilter === 'all'}
            onClick={() => updateStatus('all')}
          />
          <StatCounter
            label={t('interceptor.audit.stats.success')}
            value={stats.successful}
            active={statusFilter === 'success'}
            onClick={() => toggleStatus('success')}
          />
          <StatCounter
            label={t('interceptor.audit.stats.failed')}
            value={stats.failed}
            active={statusFilter === 'failed'}
            onClick={() => toggleStatus('failed')}
          />
          <StatCounter
            label={t('interceptor.audit.stats.blocked')}
            value={stats.blocked}
            active={statusFilter === 'blocked'}
            onClick={() => toggleStatus('blocked')}
          />
          <div className="h-4 w-px bg-border mx-1" />
          <StatCounter label={t('interceptor.audit.stats.lastHour')} value={stats.last_hour} />
        </div>
      )}

      <div className="flex-1 min-h-0 overflow-auto">
        {entries.length === 0 ? (
          <p className="px-4 py-12 text-sm text-center text-muted-foreground">
            {loading ? '' : t('interceptor.audit.noEntries')}
          </p>
        ) : (
          <table className="w-full text-xs">
            <thead className="sticky top-0 bg-muted/40 text-[11px] uppercase tracking-wider text-muted-foreground">
              <tr>
                <th className="w-2 px-2 py-2" />
                <th className="px-2 py-2 text-left font-medium">
                  {t('interceptor.audit.columns.time')}
                </th>
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
              {entries.map(entry => {
                const status = auditStatus(entry);
                return (
                  <tr
                    key={entry.id}
                    onClick={() => setSelectedEntry(entry)}
                    className="border-t border-border hover:bg-muted/30 cursor-pointer"
                  >
                    <td className="px-2 py-1.5">
                      <span
                        className={`block w-1.5 h-1.5 rounded-full ${STATUS_DOT[status]}`}
                        title={t(`interceptor.audit.status.${status}`)}
                      />
                    </td>
                    <td className="px-2 py-1.5 whitespace-nowrap text-muted-foreground tabular-nums">
                      {formatTimestamp(entry.timestamp, t)}
                    </td>
                    <td className="px-2 py-1.5">
                      <EnvironmentChip environment={entry.environment} />
                    </td>
                    <td className="px-2 py-1.5 max-w-0 w-full">
                      <div className="font-mono truncate text-foreground">
                        {entry.query_preview}
                      </div>
                      {(entry.safety_rule || entry.error) && (
                        <div
                          className={`truncate ${entry.error && !entry.safety_rule ? 'text-error' : 'text-warning'}`}
                        >
                          {entry.safety_rule
                            ? t(
                                entry.blocked
                                  ? 'interceptor.audit.blockedBy'
                                  : 'interceptor.audit.flaggedBy',
                                { rule: getSafetyRuleLabel(entry.safety_rule) }
                              )
                            : entry.error}
                        </div>
                      )}
                    </td>
                    <td className="px-2 py-1.5 whitespace-nowrap text-muted-foreground">
                      {entry.database ?? '—'}
                    </td>
                    <td className="px-2 py-1.5 text-right whitespace-nowrap tabular-nums">
                      {formatExecutionTime(entry.execution_time_ms)}
                    </td>
                    <td className="px-3 py-1.5 text-right whitespace-nowrap tabular-nums text-muted-foreground">
                      {entry.row_count ?? '—'}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      <div className="flex items-center justify-between px-3 py-1.5 border-t border-border text-xs text-muted-foreground">
        <span>{t('interceptor.audit.pagination', { page: page + 1, count: entries.length })}</span>
        <div className="flex items-center gap-1">
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => setPage(p => Math.max(0, p - 1))}
            disabled={page === 0}
          >
            <ChevronLeft size={14} />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => setPage(p => p + 1)}
            disabled={!hasMore}
          >
            <ChevronRight size={14} />
          </Button>
        </div>
      </div>

      {selectedEntry && (
        <AuditEntryDetail
          entry={selectedEntry}
          onClose={() => setSelectedEntry(null)}
          getSafetyRuleLabel={getSafetyRuleLabel}
          onFilterByFingerprint={fingerprint => {
            setFingerprintFilter(fingerprint);
            setPage(0);
          }}
        />
      )}
    </div>
  );
}
