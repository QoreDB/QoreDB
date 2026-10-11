// SPDX-License-Identifier: BUSL-1.1

import { ChevronDown, Clock, Download, History, RotateCcw, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { confirmDialog } from '@/lib/stores/confirmStore';
import { createQueryTab } from '@/lib/tabs';
import type { ChangelogEntry, Namespace, RollbackSqlResponse, TimelineEvent } from '@/lib/tauri';
import {
  clearTableChangelog,
  exportChangelog,
  generateEntryRollbackSql,
  getRowHistory,
  getTableTimeline,
  getTimeTravelConfig,
} from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { formatTimestamp, OperationBadge, PrimaryKeyDisplay } from './OperationBadge';
import { RollbackDialog } from './RollbackDialog';
import { RowHistoryPanel } from './RowHistoryPanel';

const PAGE_SIZE = 50;

interface TimeTravelViewerProps {
  sessionId: string;
  namespace?: Namespace;
  tableName?: string;
  driverId?: string | null;
  onOpenTab?: (tab: ReturnType<typeof createQueryTab>) => void;
}

export function TimeTravelViewer(props: TimeTravelViewerProps) {
  return (
    <ScopedTimeTravelViewer
      key={JSON.stringify([props.sessionId, props.namespace, props.tableName])}
      {...props}
    />
  );
}

function ScopedTimeTravelViewer({
  sessionId,
  namespace,
  tableName,
  onOpenTab,
}: TimeTravelViewerProps) {
  const { t } = useTranslation();
  const active = useRef(true);
  const timelineRequest = useRef(0);
  const historyRequest = useRef(0);
  const rollbackRequest = useRef(0);
  const [error, setError] = useState(false);

  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
      timelineRequest.current += 1;
      historyRequest.current += 1;
      rollbackRequest.current += 1;
    };
  }, []);
  const [events, setEvents] = useState<TimelineEvent[]>([]);
  const [totalCount, setTotalCount] = useState(0);
  const [loading, setLoading] = useState(false);
  const [enabled, setEnabled] = useState(true);
  const [operationFilter, setOperationFilter] = useState<string>('all');
  const [pkSearch, setPkSearch] = useState('');
  const [offset, setOffset] = useState(0);

  const [rowHistoryEntries, setRowHistoryEntries] = useState<ChangelogEntry[]>([]);
  const [showRowHistory, setShowRowHistory] = useState(false);
  const [rollbackResult, setRollbackResult] = useState<RollbackSqlResponse | null>(null);
  const [rollbackOpen, setRollbackOpen] = useState(false);

  const fetchTimeline = useCallback(
    async (newOffset = 0) => {
      if (!namespace || !tableName) return;
      const request = ++timelineRequest.current;
      setLoading(true);
      setError(false);
      try {
        const res = await getTableTimeline(
          sessionId,
          namespace.database,
          namespace.schema ?? null,
          tableName,
          {
            operation: operationFilter === 'all' ? undefined : operationFilter,
            primaryKeySearch: pkSearch || undefined,
            limit: PAGE_SIZE,
            offset: newOffset,
          }
        );
        if (!active.current || request !== timelineRequest.current) return;
        if (!res.success) throw new Error(res.error ?? 'Timeline unavailable');
        setEvents(newOffset === 0 ? res.events : prev => [...prev, ...res.events]);
        setTotalCount(res.total_count);
        setOffset(newOffset);
      } catch {
        if (active.current && request === timelineRequest.current) setError(true);
      } finally {
        if (active.current && request === timelineRequest.current) setLoading(false);
      }
    },
    [sessionId, namespace, tableName, operationFilter, pkSearch]
  );

  useEffect(() => {
    setOffset(0);
    setEvents([]);
    setTotalCount(0);
    fetchTimeline(0);
  }, [fetchTimeline]);

  useEffect(() => {
    getTimeTravelConfig()
      .then(res => res.success && setEnabled(res.config.enabled))
      .catch(() => {});
  }, []);

  const handleViewRowHistory = async (event: TimelineEvent) => {
    if (!namespace || !tableName || !event.primary_key) return;
    const request = ++historyRequest.current;
    const res = await getRowHistory(
      sessionId,
      namespace.database,
      namespace.schema ?? null,
      tableName,
      event.primary_key,
      50
    ).catch(() => null);
    if (!active.current || request !== historyRequest.current) return;
    if (res?.success) {
      setRowHistoryEntries(res.entries);
      setShowRowHistory(true);
    } else {
      toast.error(t('timeTravel.requestFailed'));
    }
  };

  const handleRollbackEntry = async (entryId: string) => {
    const request = ++rollbackRequest.current;
    const res = await generateEntryRollbackSql(sessionId, entryId).catch(() => null);
    if (!active.current || request !== rollbackRequest.current) return;
    if (res?.success) {
      setRollbackResult(res);
      setRollbackOpen(true);
    } else {
      toast.error(t('timeTravel.requestFailed'));
    }
  };

  const handleExport = async () => {
    if (!namespace || !tableName) return;
    try {
      const json = await exportChangelog(sessionId, {
        tableName,
        namespace,
        limit: 10_000,
        operation: operationFilter === 'all' ? undefined : operationFilter,
        primaryKeySearch: pkSearch || undefined,
      });
      if (!active.current) return;
      const blob = new Blob([json], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${tableName}-changelog.json`;
      a.click();
      URL.revokeObjectURL(url);
    } catch {
      if (active.current) toast.error(t('timeTravel.requestFailed'));
    }
  };

  const handleClear = async () => {
    if (!namespace || !tableName) return;
    if (!(await confirmDialog({ description: t('timeTravel.toolbar.clearConfirm') }))) return;
    if (!active.current) return;
    try {
      const res = await clearTableChangelog(
        sessionId,
        namespace.database,
        namespace.schema ?? null,
        tableName
      );
      if (!active.current) return;
      if (!res.success) throw new Error(res.error ?? 'Clear failed');
      historyRequest.current += 1;
      rollbackRequest.current += 1;
      setShowRowHistory(false);
      setRowHistoryEntries([]);
      setRollbackOpen(false);
      setRollbackResult(null);
      await fetchTimeline(0);
    } catch {
      if (active.current) toast.error(t('timeTravel.requestFailed'));
    }
  };

  // Disabled state
  if (!enabled) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3 text-muted-foreground">
        <History size={48} className="opacity-30" />
        <h2 className="text-lg font-medium">{t('timeTravel.settings.disabled')}</h2>
        <p className="text-sm">{t('timeTravel.settings.disabledDescription')}</p>
      </div>
    );
  }

  // Empty state
  if (
    !error &&
    !loading &&
    events.length === 0 &&
    offset === 0 &&
    operationFilter === 'all' &&
    !pkSearch
  ) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3 text-muted-foreground mt-12">
        <History size={48} className="opacity-30" />
        <h2 className="text-lg font-medium">{t('timeTravel.empty.title')}</h2>
        <p className="text-sm max-w-md text-center">{t('timeTravel.empty.description')}</p>
        <p className="text-xs opacity-60">{t('timeTravel.empty.hint')}</p>
        <p className="text-xs opacity-60">{t('timeTravel.scopeHint')}</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      {/* Toolbar */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border">
        <History size={16} className="text-muted-foreground" />
        <h2 className="text-sm font-medium flex-1">
          {t('timeTravel.title')} — {tableName}
        </h2>
        <span className="text-xs text-muted-foreground">
          {t('timeTravel.timeline.events', { count: totalCount })}
        </span>
        <Button variant="ghost" size="sm" onClick={handleExport}>
          <Download size={14} className="mr-1" />
          {t('timeTravel.toolbar.export')}
        </Button>
        <Button variant="ghost" size="sm" onClick={handleClear}>
          <Trash2 size={14} className="mr-1" />
          {t('timeTravel.toolbar.clear')}
        </Button>
      </div>

      <p className="px-3 py-1 text-xs text-muted-foreground">{t('timeTravel.scopeHint')}</p>
      {error && (
        <div role="alert" className="px-3 py-2 text-sm text-destructive">
          {t('timeTravel.requestFailed')}
          <Button variant="ghost" size="sm" onClick={() => fetchTimeline(0)}>
            {t('timeTravel.retry')}
          </Button>
        </div>
      )}

      {/* Filters */}
      <div className="flex items-center gap-2 px-3 py-1.5 border-b border-border">
        <Select value={operationFilter} onValueChange={setOperationFilter}>
          <SelectTrigger className="h-7 w-40 text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t('timeTravel.filters.allOperations')}</SelectItem>
            <SelectItem value="insert">INSERT</SelectItem>
            <SelectItem value="update">UPDATE</SelectItem>
            <SelectItem value="delete">DELETE</SelectItem>
          </SelectContent>
        </Select>
        <Input
          placeholder={t('timeTravel.filters.searchPk')}
          value={pkSearch}
          onChange={e => setPkSearch(e.target.value)}
          className="h-7 text-xs max-w-60"
        />
      </div>

      {/* Content */}
      <div className="flex flex-1 overflow-hidden">
        <ScrollArea className={cn('flex-1', showRowHistory && 'border-r border-border')}>
          <div className="divide-y divide-border">
            {events.map(event => (
              <TimelineEventRow
                key={event.entry_id}
                event={event}
                onViewHistory={handleViewRowHistory}
                onRollback={handleRollbackEntry}
                t={t}
              />
            ))}
          </div>

          {events.length < totalCount && (
            <div className="flex justify-center p-3">
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  const next = offset + PAGE_SIZE;
                  fetchTimeline(next);
                }}
                disabled={loading}
              >
                <ChevronDown size={14} className="mr-1" />
                {t('timeTravel.timeline.loadMore')}
              </Button>
            </div>
          )}
        </ScrollArea>

        {showRowHistory && (
          <div className="w-96 shrink-0">
            <RowHistoryPanel
              entries={rowHistoryEntries}
              onClose={() => setShowRowHistory(false)}
              onRollback={entry => handleRollbackEntry(entry.id)}
            />
          </div>
        )}
      </div>

      <RollbackDialog
        open={rollbackOpen}
        result={rollbackResult}
        onClose={() => setRollbackOpen(false)}
        onCopy={() => rollbackResult?.sql && navigator.clipboard.writeText(rollbackResult.sql)}
        onOpenInQueryTab={() => {
          if (rollbackResult?.sql && onOpenTab) {
            onOpenTab(createQueryTab(rollbackResult.sql, namespace));
            setRollbackOpen(false);
          }
        }}
      />
    </div>
  );
}

// ─── Timeline row (extracted to keep the main component focused) ───────────

function TimelineEventRow({
  event,
  onViewHistory,
  onRollback,
  t,
}: {
  event: TimelineEvent;
  onViewHistory: (e: TimelineEvent) => void;
  onRollback: (entryId: string) => void;
  t: (key: string) => string;
}) {
  return (
    <div className="flex items-center gap-3 px-3 py-1.5 hover:bg-muted/30 text-xs group">
      <Clock size={12} className="text-muted-foreground shrink-0" />
      <span className="text-muted-foreground w-28 shrink-0 font-mono">
        {formatTimestamp(event.timestamp)}
      </span>
      <OperationBadge operation={event.operation} />
      <PrimaryKeyDisplay pk={event.primary_key} />
      {event.connection_name && (
        <span className="text-muted-foreground ml-auto">{event.connection_name}</span>
      )}
      <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity ml-auto shrink-0">
        {event.primary_key && (
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6"
            onClick={() => onViewHistory(event)}
            title={t('timeTravel.rowHistory.title')}
          >
            <History size={12} />
          </Button>
        )}
        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6"
          onClick={() => onRollback(event.entry_id)}
          title={t('timeTravel.rollback.rollbackToPoint')}
        >
          <RotateCcw size={12} />
        </Button>
      </div>
    </div>
  );
}
