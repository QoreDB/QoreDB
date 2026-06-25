// SPDX-License-Identifier: Apache-2.0

/**
 * Audit Log Panel
 *
 * UI component for viewing and filtering the query audit log.
 * All data is fetched from the backend (Rust) for security.
 */

import type { TFunction } from 'i18next';
import {
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  Clock,
  Hash,
  RefreshCw,
  Search,
  Shield,
  Trash2,
  XCircle,
} from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { LicenseBadge } from '@/components/License/LicenseBadge';
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
import { ScrollArea } from '../ui/scroll-area';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import { AuditEntryDetail } from './AuditEntryDetail';
import { AuditExportMenu } from './AuditExportMenu';

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
            {entry.fingerprint && (
              <span className="flex items-center gap-1 font-mono">
                <Hash className="w-3 h-3" />
                {entry.fingerprint.slice(0, 8)}
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
  active?: boolean;
  onClick?: () => void;
}

function StatsCard({ label, value, color = 'text-foreground', active, onClick }: StatsCardProps) {
  const baseClass = 'p-3 rounded-lg border text-left transition-colors';
  const stateClass = active
    ? 'bg-accent/10 border-accent/40 ring-1 ring-accent/30'
    : 'bg-muted/50 border-border hover:bg-muted';

  if (!onClick) {
    return (
      <div className={`${baseClass} bg-muted/50 border-border`}>
        <p className="text-xs text-muted-foreground">{label}</p>
        <p className={`text-lg font-semibold ${color}`}>{value.toLocaleString()}</p>
      </div>
    );
  }

  return (
    <button type="button" onClick={onClick} className={`${baseClass} ${stateClass}`}>
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className={`text-lg font-semibold ${color}`}>{value.toLocaleString()}</p>
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
  const [fingerprintFilter, setFingerprintFilter] = useState<string | null>(null);

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
      } else if (statusFilter === 'blocked') {
        filter.blocked = true;
      }

      if (fingerprintFilter) {
        filter.fingerprint = fingerprintFilter;
      }

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
  }, [page, search, environmentFilter, statusFilter, fingerprintFilter]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Reset page when filters change
  const updateSearch = useCallback((value: string) => {
    setSearch(value);
    setPage(0);
  }, []);

  const updateEnvironment = useCallback((value: Environment | 'all') => {
    setEnvironmentFilter(value);
    setPage(0);
  }, []);

  const updateStatus = useCallback((value: typeof statusFilter) => {
    setStatusFilter(value);
    setPage(0);
  }, []);

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

  const handleFilterByFingerprint = useCallback((fingerprint: string) => {
    setFingerprintFilter(fingerprint);
    setPage(0);
  }, []);

  const clearFingerprintFilter = useCallback(() => {
    setFingerprintFilter(null);
    setPage(0);
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
              <AuditExportMenu />
              <Button variant="outline" size="sm" onClick={handleClear}>
                <Trash2 className="w-4 h-4 mr-1" />
                {t('interceptor.audit.clearLog')}
              </Button>
            </>
          )}
        </div>
      </div>

      {/* Stats — Pro only. Click a status card to filter the list. */}
      {isAdvanced && stats && (
        <div className="grid grid-cols-5 gap-2 p-4 border-b border-border">
          <StatsCard
            label={t('interceptor.audit.stats.total')}
            value={stats.total}
            active={statusFilter === 'all'}
            onClick={() => updateStatus('all')}
          />
          <StatsCard
            label={t('interceptor.audit.stats.success')}
            value={stats.successful}
            color="text-green-500"
            active={statusFilter === 'success'}
            onClick={() => updateStatus(statusFilter === 'success' ? 'all' : 'success')}
          />
          <StatsCard
            label={t('interceptor.audit.stats.failed')}
            value={stats.failed}
            color="text-red-500"
            active={statusFilter === 'failed'}
            onClick={() => updateStatus(statusFilter === 'failed' ? 'all' : 'failed')}
          />
          <StatsCard
            label={t('interceptor.audit.stats.blocked')}
            value={stats.blocked}
            color="text-yellow-500"
            active={statusFilter === 'blocked'}
            onClick={() => updateStatus(statusFilter === 'blocked' ? 'all' : 'blocked')}
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
              onChange={e => updateSearch(e.target.value)}
              className="pl-9"
            />
          </div>

          <Select
            value={environmentFilter}
            onValueChange={v => updateEnvironment(v as Environment | 'all')}
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

          <Select value={statusFilter} onValueChange={v => updateStatus(v as typeof statusFilter)}>
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

      {isAdvanced && fingerprintFilter && (
        <div className="flex items-center gap-2 px-4 py-2 border-b border-border bg-muted/30 text-xs">
          <Hash className="w-3 h-3 text-muted-foreground" />
          <span className="text-muted-foreground">{t('interceptor.audit.fingerprintFilter')}</span>
          <code className="font-mono px-2 py-0.5 rounded bg-background border border-border">
            {fingerprintFilter}
          </code>
          <Button
            variant="ghost"
            size="sm"
            className="h-6 px-2 text-xs"
            onClick={clearFingerprintFilter}
          >
            {t('common.clear')}
          </Button>
        </div>
      )}

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

      {selectedEntry && (
        <AuditEntryDetail
          entry={selectedEntry}
          onClose={() => setSelectedEntry(null)}
          getSafetyRuleLabel={getSafetyRuleLabel}
          onFilterByFingerprint={handleFilterByFingerprint}
        />
      )}
    </div>
  );
}
