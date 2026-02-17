// SPDX-License-Identifier: Apache-2.0

/**
 * Audit Log Panel
 *
 * UI component for viewing and filtering the query audit log.
 * All data is fetched from the backend (Rust) for security.
 */

import { useState, useCallback, useEffect } from 'react';
import type { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';
import {
  Search,
  Download,
  Trash2,
  ChevronLeft,
  ChevronRight,
  Shield,
  CheckCircle2,
  XCircle,
  Clock,
  RefreshCw,
  X,
} from 'lucide-react';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import { ScrollArea } from '../ui/scroll-area';
import { useLicense } from '@/providers/LicenseProvider';
import { LicenseBadge } from '@/components/License/LicenseBadge';
import {
  getAuditEntries,
  getAuditStats,
  clearAuditLog,
  exportAuditLog,
  formatExecutionTime,
  type AuditLogEntry,
  type AuditStats,
  type AuditFilter,
  type Environment,
  BUILTIN_SAFETY_RULE_I18N,
} from '../../lib/tauri/interceptor';

const PAGE_SIZE = 50;

function formatTimestamp(timestamp: string, t: TFunction): string {
  const date = new Date(timestamp);
  const now = new Date();
  const diff = now.getTime() - date.getTime();

  if (diff < 60000) return t('interceptor.audit.time.justNow');
  if (diff < 3600000) {
    return t('interceptor.audit.time.minutesAgo', {
      count: Math.floor(diff / 60000),
    });
  }
  if (diff < 86400000) {
    return t('interceptor.audit.time.hoursAgo', {
      count: Math.floor(diff / 3600000),
    });
  }
  if (diff < 604800000) {
    return t('interceptor.audit.time.daysAgo', {
      count: Math.floor(diff / 86400000),
    });
  }

  return date.toLocaleDateString();
}

function getEnvironmentColor(env: Environment): string {
  switch (env) {
    case 'development':
      return 'bg-green-500/10 text-green-600';
    case 'staging':
      return 'bg-yellow-500/10 text-yellow-600';
    case 'production':
      return 'bg-red-500/10 text-red-600';
    default:
      return 'bg-muted text-muted-foreground';
  }
}

interface AuditEntryItemProps {
  entry: AuditLogEntry;
  onSelect?: (entry: AuditLogEntry) => void;
  getSafetyRuleLabel?: (ruleId?: string | null) => string;
}

function AuditEntryItem({ entry, onSelect, getSafetyRuleLabel }: AuditEntryItemProps) {
  const { t } = useTranslation();

  const StatusIcon = entry.blocked ? Shield : entry.success ? CheckCircle2 : XCircle;

  const statusColor = entry.blocked
    ? 'text-yellow-500'
    : entry.success
      ? 'text-green-500'
      : 'text-red-500';

  return (
    <button
      type="button"
      className="w-full text-left p-3 rounded-lg border border-border hover:bg-muted/50 transition-colors"
      onClick={() => onSelect?.(entry)}
    >
      <div className="flex items-start gap-3">
        <StatusIcon className={`w-4 h-4 mt-0.5 shrink-0 ${statusColor}`} />

        <div className="flex-1 min-w-0 space-y-1">
          <div className="flex items-center gap-2 flex-wrap">
            <span
              className={`text-xs px-1.5 py-0.5 rounded font-medium ${getEnvironmentColor(entry.environment)}`}
            >
              {entry.environment.toUpperCase()}
            </span>
            <span className="text-xs text-muted-foreground">{entry.operation_type}</span>
            <span className="text-xs text-muted-foreground">{entry.driver_id}</span>
          </div>

          <p className="text-sm font-mono truncate text-foreground">{entry.query_preview}</p>

          <div className="flex items-center gap-3 text-xs text-muted-foreground">
            <span>{formatTimestamp(entry.timestamp, t)}</span>
            {entry.database && <span>{entry.database}</span>}
            <span className="flex items-center gap-1">
              <Clock className="w-3 h-3" />
              {formatExecutionTime(entry.execution_time_ms)}
            </span>
            {entry.row_count != null && (
              <span>
                {entry.row_count} {t('table.rows')}
              </span>
            )}
          </div>

          {entry.blocked && entry.safety_rule && (
            <p className="text-xs text-yellow-600 dark:text-yellow-400">
              {t('interceptor.audit.blockedBy', {
                rule: getSafetyRuleLabel?.(entry.safety_rule) ?? entry.safety_rule,
              })}
            </p>
          )}

          {entry.error && <p className="text-xs text-red-500 truncate">{entry.error}</p>}
        </div>
      </div>
    </button>
  );
}

interface StatsCardProps {
  label: string;
  value: number;
  color?: string;
}

function StatsCard({ label, value, color = 'text-foreground' }: StatsCardProps) {
  return (
    <div className="p-3 rounded-lg bg-muted/50 border border-border">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className={`text-lg font-semibold ${color}`}>{value.toLocaleString()}</p>
    </div>
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
      if (keys) {
        return t(keys.nameKey);
      }
      return ruleId;
    },
    [t]
  );

  // State
  const [entries, setEntries] = useState<AuditLogEntry[]>([]);
  const [stats, setStats] = useState<AuditStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [page, setPage] = useState(0);
  const [selectedEntry, setSelectedEntry] = useState<AuditLogEntry | null>(null);

  // Filters
  const [search, setSearch] = useState('');
  const [environmentFilter, setEnvironmentFilter] = useState<Environment | 'all'>('all');
  const [statusFilter, setStatusFilter] = useState<'all' | 'success' | 'failed' | 'blocked'>('all');

  // Load data from backend
  const loadData = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);

      const filter: AuditFilter = {
        limit: PAGE_SIZE,
        offset: page * PAGE_SIZE,
      };

      if (environmentFilter !== 'all') {
        filter.environment = environmentFilter;
      }

      if (search.trim()) {
        filter.search = search.trim();
      }

      if (statusFilter === 'success') {
        filter.success = true;
      } else if (statusFilter === 'failed') {
        filter.success = false;
      }
      // Note: blocked filter not directly supported, would need to add to backend

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
  }, [page, search, environmentFilter, statusFilter]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Reset page when filters change
  useEffect(() => {
    setPage(0);
  }, [search, environmentFilter, statusFilter]);

  const handleClear = useCallback(async () => {
    if (window.confirm(t('interceptor.audit.clearConfirm'))) {
      try {
        await clearAuditLog();
        loadData();
      } catch (err) {
        console.error('Failed to clear audit log:', err);
      }
    }
  }, [t, loadData]);

  const handleExport = useCallback(async () => {
    try {
      const content = await exportAuditLog();
      const blob = new Blob([content], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `qoredb-audit-log-${new Date().toISOString().split('T')[0]}.json`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      console.error('Failed to export audit log:', err);
    }
  }, []);

  const hasMore = entries.length === PAGE_SIZE;

  if (loading && entries.length === 0) {
    return (
      <div className="flex items-center justify-center p-8">
        <RefreshCw className="w-5 h-5 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-4 text-center">
        <p className="text-destructive mb-2">{error}</p>
        <Button variant="outline" size="sm" onClick={loadData}>
          {t('common.retry')}
        </Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b border-border">
        <h2 className="text-lg font-semibold">{t('interceptor.audit.title')}</h2>
        <div className="flex items-center gap-2">
          <Button variant="ghost" size="icon" onClick={loadData} disabled={loading}>
            <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
          </Button>
          {isAdvanced && (
            <>
              <Button variant="outline" size="sm" onClick={handleExport}>
                <Download className="w-4 h-4 mr-1" />
                JSON
              </Button>
              <Button variant="outline" size="sm" onClick={handleClear}>
                <Trash2 className="w-4 h-4 mr-1" />
                {t('interceptor.audit.clearLog')}
              </Button>
            </>
          )}
        </div>
      </div>

      {/* Stats — Pro only */}
      {isAdvanced && stats && (
        <div className="grid grid-cols-5 gap-2 p-4 border-b border-border">
          <StatsCard label={t('interceptor.audit.stats.total')} value={stats.total} />
          <StatsCard
            label={t('interceptor.audit.stats.success')}
            value={stats.successful}
            color="text-green-500"
          />
          <StatsCard
            label={t('interceptor.audit.stats.failed')}
            value={stats.failed}
            color="text-red-500"
          />
          <StatsCard
            label={t('interceptor.audit.stats.blocked')}
            value={stats.blocked}
            color="text-yellow-500"
          />
          <StatsCard
            label={t('interceptor.audit.stats.lastHour')}
            value={stats.last_hour}
            color="text-blue-500"
          />
        </div>
      )}

      {/* Filters — Pro only */}
      {isAdvanced ? (
        <div className="flex items-center gap-2 p-4 border-b border-border">
          <div className="relative flex-1">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
            <Input
              placeholder={t('search.placeholder')}
              value={search}
              onChange={e => setSearch(e.target.value)}
              className="pl-9"
            />
          </div>

          <Select
            value={environmentFilter}
            onValueChange={v => setEnvironmentFilter(v as Environment | 'all')}
          >
            <SelectTrigger className="w-40">
              <SelectValue placeholder={t('interceptor.audit.filters.environment')} />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t('interceptor.audit.filters.allEnvironments')}</SelectItem>
              <SelectItem value="development">{t('environment.development')}</SelectItem>
              <SelectItem value="staging">{t('environment.staging')}</SelectItem>
              <SelectItem value="production">{t('environment.production')}</SelectItem>
            </SelectContent>
          </Select>

          <Select value={statusFilter} onValueChange={v => setStatusFilter(v as typeof statusFilter)}>
            <SelectTrigger className="w-32">
              <SelectValue placeholder={t('interceptor.audit.filters.status')} />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t('interceptor.audit.filters.all')}</SelectItem>
              <SelectItem value="success">{t('interceptor.audit.status.success')}</SelectItem>
              <SelectItem value="failed">{t('interceptor.audit.status.failed')}</SelectItem>
              <SelectItem value="blocked">{t('interceptor.audit.status.blocked')}</SelectItem>
            </SelectContent>
          </Select>
        </div>
      ) : (
        <div className="flex items-center gap-2 px-4 py-2 border-b border-border text-xs text-muted-foreground">
          <LicenseBadge tier="pro" />
          <span>{t('interceptor.audit.upgradeForFilters')}</span>
        </div>
      )}

      {/* Entries */}
      <ScrollArea className="flex-1">
        <div className="p-4 space-y-2">
          {entries.length === 0 ? (
            <p className="text-center text-muted-foreground py-8">
              {t('interceptor.audit.noEntries')}
            </p>
          ) : (
            entries.map(entry => (
              <AuditEntryItem
                key={entry.id}
                entry={entry}
                onSelect={setSelectedEntry}
                getSafetyRuleLabel={getSafetyRuleLabel}
              />
            ))
          )}
        </div>
      </ScrollArea>

      {/* Pagination */}
      <div className="flex items-center justify-between p-4 border-t border-border">
        <p className="text-sm text-muted-foreground">
          {t('interceptor.audit.pagination', {
            page: page + 1,
            count: entries.length,
          })}
        </p>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setPage(p => Math.max(0, p - 1))}
            disabled={page === 0}
          >
            <ChevronLeft className="w-4 h-4" />
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setPage(p => p + 1)}
            disabled={!hasMore}
          >
            <ChevronRight className="w-4 h-4" />
          </Button>
        </div>
      </div>

      {/* Entry Detail Modal */}
      {selectedEntry && (
        <AuditEntryDetail
          entry={selectedEntry}
          onClose={() => setSelectedEntry(null)}
          getSafetyRuleLabel={getSafetyRuleLabel}
        />
      )}
    </div>
  );
}

interface AuditEntryDetailProps {
  entry: AuditLogEntry;
  onClose: () => void;
  getSafetyRuleLabel?: (ruleId?: string | null) => string;
}

function AuditEntryDetail({ entry, onClose, getSafetyRuleLabel }: AuditEntryDetailProps) {
  const { t } = useTranslation();

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={onClose} />

      <div className="relative bg-background rounded-lg shadow-xl border border-border w-full max-w-2xl mx-4 max-h-[80vh] overflow-hidden flex flex-col">
        <div className="flex items-center justify-between p-4 border-b border-border">
          <h3 className="font-semibold">{t('interceptor.audit.detail.title')}</h3>
          <button
            type="button"
            onClick={onClose}
            className="p-1 rounded hover:bg-muted transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        <ScrollArea className="flex-1 p-4">
          <div className="space-y-4">
            {/* Status */}
            <div className="flex items-center gap-2 flex-wrap">
              <span
                className={`text-xs px-2 py-1 rounded font-medium ${getEnvironmentColor(entry.environment)}`}
              >
                {entry.environment.toUpperCase()}
              </span>
              <span className="text-xs px-2 py-1 rounded bg-muted">{entry.operation_type}</span>
              <span className="text-xs px-2 py-1 rounded bg-muted">{entry.driver_id}</span>
              <span
                className={`text-xs px-2 py-1 rounded ${
                  entry.blocked
                    ? 'bg-yellow-500/10 text-yellow-600'
                    : entry.success
                      ? 'bg-green-500/10 text-green-600'
                      : 'bg-red-500/10 text-red-600'
                }`}
              >
                {entry.blocked
                  ? t('interceptor.audit.status.blocked')
                  : entry.success
                    ? t('interceptor.audit.status.success')
                    : t('interceptor.audit.status.failed')}
              </span>
            </div>

            {/* Query */}
            <div>
              <Label className="text-sm font-medium">{t('interceptor.audit.detail.query')}</Label>
              <pre className="mt-1 p-3 rounded bg-muted font-mono text-sm whitespace-pre-wrap break-all">
                {entry.query}
              </pre>
            </div>

            {/* Details Grid */}
            <div className="grid grid-cols-2 gap-4 text-sm">
              <div>
                <Label className="text-muted-foreground">
                  {t('interceptor.audit.detail.timestamp')}
                </Label>
                <p>{new Date(entry.timestamp).toLocaleString()}</p>
              </div>
              <div>
                <Label className="text-muted-foreground">
                  {t('interceptor.audit.detail.sessionId')}
                </Label>
                <p className="font-mono text-xs">{entry.session_id}</p>
              </div>
              <div>
                <Label className="text-muted-foreground">
                  {t('interceptor.audit.detail.database')}
                </Label>
                <p>{entry.database || '-'}</p>
              </div>
              <div>
                <Label className="text-muted-foreground">
                  {t('interceptor.audit.detail.executionTime')}
                </Label>
                <p>{formatExecutionTime(entry.execution_time_ms)}</p>
              </div>
              {entry.row_count != null && (
                <div>
                  <Label className="text-muted-foreground">
                    {t('interceptor.audit.detail.rowCount')}
                  </Label>
                  <p>{entry.row_count}</p>
                </div>
              )}
            </div>

            {/* Safety Rule */}
            {entry.blocked && entry.safety_rule && (
              <div>
                <Label className="text-sm font-medium text-yellow-600">
                  {t('interceptor.audit.detail.blockedBy')}
                </Label>
                <p className="mt-1 text-sm text-yellow-600">
                  {getSafetyRuleLabel?.(entry.safety_rule) ?? entry.safety_rule}
                </p>
              </div>
            )}

            {/* Error */}
            {entry.error && (
              <div>
                <Label className="text-sm font-medium text-red-600">
                  {t('interceptor.audit.detail.error')}
                </Label>
                <p className="mt-1 text-sm text-red-600">{entry.error}</p>
              </div>
            )}
          </div>
        </ScrollArea>

        <div className="flex justify-end p-4 border-t border-border">
          <Button variant="outline" onClick={onClose}>
            {t('common.close')}
          </Button>
        </div>
      </div>
    </div>
  );
}
