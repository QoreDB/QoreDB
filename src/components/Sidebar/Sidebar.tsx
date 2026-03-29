// SPDX-License-Identifier: Apache-2.0

import { Bug, Database, Plus, Search } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { LicenseBadge } from '@/components/License/LicenseBadge';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import { Button } from '@/components/ui/button';
import { useTheme } from '@/hooks/useTheme';
import {
  reconcileFavoriteConnectionIds,
  saveFavoriteConnectionIds,
} from '@/lib/connectionFavorites';
import { setLogsOpen, useModalStore } from '@/lib/modalStore';
import { UI_EVENT_CONNECTIONS_CHANGED } from '@/lib/uiEvents';
import { useLicense } from '@/providers/LicenseProvider';
import {
  type Collection,
  connectSavedConnection,
  type DatabaseEvent,
  listSavedConnections,
  type Namespace,
  type RelationFilter,
  type Routine,
  type SavedConnection,
  type Sequence,
  type Trigger,
} from '../../lib/tauri';
import { ErrorLogPanel } from '../Logs/ErrorLogPanel';
import { DBTree } from '../Tree/DBTree';
import { ConnectionItem } from './ConnectionItem';

const DEFAULT_PROJECT = 'default';

interface SidebarProps {
  onNewConnection: () => void;
  onConnected: (sessionId: string, connection: SavedConnection) => void;
  connectedSessionId: string | null;
  connectedConnectionId?: string | null;
  onTableSelect?: (
    namespace: Namespace,
    tableName: string,
    relationFilter?: RelationFilter
  ) => void;
  onDatabaseSelect?: (namespace: Namespace) => void;
  onCompareTable?: (collection: Collection) => void;
  onAiGenerateForTable?: (collection: Collection) => void;
  onOpenRoutineSource?: (routine: Routine, namespace: Namespace) => void;
  onCreateRoutine?: (routineType: 'Function' | 'Procedure', namespace: Namespace) => void;
  onOpenTriggerSource?: (trigger: Trigger, namespace: Namespace) => void;
  onCreateTrigger?: (namespace: Namespace) => void;
  onOpenEventSource?: (event: DatabaseEvent, namespace: Namespace) => void;
  onCreateEvent?: (namespace: Namespace) => void;
  onOpenSequenceSource?: (sequence: Sequence, namespace: Namespace) => void;
  onEditConnection: (connection: SavedConnection, password: string) => void;
  onNewQuery?: () => void;
  onNewNotebook?: () => void;
  schemaRefreshTrigger?: number;
  activeNamespace?: Namespace | null;
  style?: React.CSSProperties;
}

export function Sidebar({
  onNewConnection,
  onConnected,
  connectedSessionId,
  connectedConnectionId,
  onTableSelect,
  onDatabaseSelect,
  onCompareTable,
  onAiGenerateForTable,
  onOpenRoutineSource,
  onCreateRoutine,
  onOpenTriggerSource,
  onCreateTrigger,
  onOpenEventSource,
  onCreateEvent,
  onOpenSequenceSource,
  onEditConnection,
  onNewQuery,
  onNewNotebook,
  schemaRefreshTrigger,
  activeNamespace,
  style,
}: SidebarProps) {
  const [connections, setConnections] = useState<SavedConnection[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [connecting, setConnecting] = useState<string | null>(null);
  const [favoriteConnectionIds, setFavoriteConnectionIds] = useState<string[]>([]);
  const [searchFilter, setSearchFilter] = useState('');

  const { t } = useTranslation();
  const { resolvedTheme } = useTheme();
  const { tier } = useLicense();
  const logsOpen = useModalStore(s => s.logsOpen);

  const loadConnections = useCallback(async () => {
    try {
      const saved = await listSavedConnections(DEFAULT_PROJECT);
      setConnections(saved);
      setFavoriteConnectionIds(
        reconcileFavoriteConnectionIds(saved.map(connection => connection.id))
      );
    } catch (err) {
      console.error('Failed to load connections:', err);
    }
  }, []);

  useEffect(() => {
    loadConnections();
  }, [loadConnections]);

  useEffect(() => {
    if (connectedConnectionId) {
      setSelectedId(connectedConnectionId);
      setExpandedId(connectedConnectionId);
    }
  }, [connectedConnectionId]);

  useEffect(() => {
    const handler = () => loadConnections();
    window.addEventListener(UI_EVENT_CONNECTIONS_CHANGED, handler);
    return () => window.removeEventListener(UI_EVENT_CONNECTIONS_CHANGED, handler);
  }, [loadConnections]);

  const favoriteConnectionSet = useMemo(
    () => new Set(favoriteConnectionIds),
    [favoriteConnectionIds]
  );

  const connectionsById = useMemo(
    () => new Map(connections.map(connection => [connection.id, connection])),
    [connections]
  );

  const filterLower = searchFilter.toLowerCase();

  const favoriteConnections = useMemo(
    () =>
      favoriteConnectionIds
        .map(connectionId => connectionsById.get(connectionId))
        .filter((connection): connection is SavedConnection => Boolean(connection))
        .filter(c => !filterLower || c.name.toLowerCase().includes(filterLower)),
    [favoriteConnectionIds, connectionsById, filterLower]
  );

  const regularConnections = useMemo(
    () =>
      connections
        .filter(connection => !favoriteConnectionSet.has(connection.id))
        .filter(c => !filterLower || c.name.toLowerCase().includes(filterLower)),
    [connections, favoriteConnectionSet, filterLower]
  );

  function handleToggleFavorite(connectionId: string) {
    setFavoriteConnectionIds(previous => {
      const next = previous.includes(connectionId)
        ? previous.filter(id => id !== connectionId)
        : [connectionId, ...previous];

      saveFavoriteConnectionIds(next);
      return next;
    });
  }

  async function handleConnect(conn: SavedConnection) {
    setConnecting(conn.id);
    setSelectedId(conn.id);

    try {
      const result = await connectSavedConnection(DEFAULT_PROJECT, conn.id);

      if (result.success && result.session_id) {
        toast.success(t('sidebar.connectedTo', { name: conn.name }));

        AnalyticsService.capture('connected_success', {
          source: 'sidebar',
          driver: conn.driver,
        });

        onConnected(result.session_id, {
          ...conn,
          environment: conn.environment,
          read_only: conn.read_only,
        });
        setExpandedId(conn.id);
      } else {
        AnalyticsService.capture('connected_failed', {
          source: 'sidebar',
          driver: conn.driver,
        });
        toast.error(t('sidebar.connectionToFailed', { name: conn.name }), {
          description: result.error || t('common.unknownError'),
        });
      }
    } catch (err) {
      AnalyticsService.capture('connected_failed', {
        source: 'sidebar',
        driver: conn.driver,
      });
      toast.error(t('sidebar.connectError'), {
        description: err instanceof Error ? err.message : t('common.unknownError'),
      });
    } finally {
      setConnecting(null);
    }
  }

  function handleSelect(conn: SavedConnection) {
    if (connectedSessionId && selectedId === conn.id) {
      setExpandedId(expandedId === conn.id ? null : conn.id);
    } else {
      handleConnect(conn);
    }
  }

  function renderConnection(connection: SavedConnection) {
    return (
      <div key={connection.id}>
        <ConnectionItem
          connection={connection}
          isSelected={selectedId === connection.id}
          isExpanded={expandedId === connection.id}
          isConnected={connectedConnectionId === connection.id}
          isConnecting={connecting === connection.id}
          isFavorite={favoriteConnectionSet.has(connection.id)}
          onSelect={() => handleSelect(connection)}
          onToggleFavorite={() => handleToggleFavorite(connection.id)}
          onEdit={onEditConnection}
          onDeleted={loadConnections}
          onNewQuery={connectedConnectionId === connection.id ? onNewQuery : undefined}
          onNewNotebook={connectedConnectionId === connection.id ? onNewNotebook : undefined}
        />
        {connecting === connection.id && (
          <div className="pl-4 border-l-2 border-accent/30 ml-4 mt-1 bg-muted/20 rounded-r-md py-2 px-3 space-y-2">
            <div className="h-3 w-3/4 rounded bg-muted animate-pulse" />
            <div className="h-3 w-1/2 rounded bg-muted animate-pulse" />
            <div className="h-3 w-2/3 rounded bg-muted animate-pulse" />
          </div>
        )}
        {expandedId === connection.id && connectedSessionId && (
          <div className="pl-4 border-l-2 border-accent/30 ml-4 mt-1 bg-muted/20 rounded-r-md py-1">
            <DBTree
              connectionId={connectedSessionId}
              driver={connection.driver}
              connection={connection}
              onTableSelect={onTableSelect}
              onDatabaseSelect={onDatabaseSelect}
              onCompareTable={onCompareTable}
              onAiGenerateForTable={onAiGenerateForTable}
              onOpenRoutineSource={onOpenRoutineSource}
              onCreateRoutine={onCreateRoutine}
              onOpenTriggerSource={onOpenTriggerSource}
              onCreateTrigger={onCreateTrigger}
              onOpenEventSource={onOpenEventSource}
              onCreateEvent={onCreateEvent}
              onOpenSequenceSource={onOpenSequenceSource}
              refreshTrigger={schemaRefreshTrigger}
              activeNamespace={activeNamespace}
            />
          </div>
        )}
      </div>
    );
  }

  return (
    <aside
      className="h-full flex flex-col border-r border-border bg-muted/30"
      style={style}
      data-allow-webview-shortcuts
    >
      <header className="h-12 flex items-center px-4 border-b border-border bg-muted/10">
        <button
          type="button"
          onClick={() => (window.location.href = '/')}
          className="flex items-center gap-2.5 font-medium text-foreground/90 hover:text-foreground transition-colors"
        >
          <img
            src={resolvedTheme === 'dark' ? '/logo-white.png' : '/logo.png'}
            alt="QoreDB"
            width={22}
            height={22}
            className="opacity-90"
          />
          <span className="text-sm tracking-tight">QoreDB</span>
          <LicenseBadge tier={tier} />
        </button>
      </header>

      <section className="flex-1 overflow-y-auto overflow-x-hidden py-2">
        {connections.length > 3 && (
          <div className="px-3 pb-2">
            <div className="relative">
              <Search
                size={14}
                className="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground/50"
              />
              <input
                type="text"
                value={searchFilter}
                onChange={e => setSearchFilter(e.target.value)}
                placeholder={t('sidebar.filterConnections')}
                className="w-full h-7 pl-8 pr-2 text-xs rounded-md border border-border bg-background text-foreground placeholder:text-muted-foreground/50 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--q-accent)]"
              />
            </div>
          </div>
        )}
        <div className="px-2 space-y-0.5 mt-1">
          {connections.length === 0 ? (
            <div className="flex flex-col items-center gap-2 px-4 py-8 text-center">
              <Database size={24} className="text-muted-foreground/40" />
              <p className="text-sm text-muted-foreground">{t('sidebar.noConnections')}</p>
              <Button variant="outline" size="sm" onClick={onNewConnection} className="mt-1">
                <Plus size={14} className="mr-1.5" />
                {t('sidebar.newConnection')}
              </Button>
            </div>
          ) : (
            <>
              {favoriteConnections.length > 0 && (
                <>
                  <div className="px-2 pb-1 pt-0.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/80">
                    {t('sidebar.favorites')}
                  </div>
                  {favoriteConnections.map(renderConnection)}
                </>
              )}

              {regularConnections.length > 0 && favoriteConnections.length > 0 && (
                <>
                  <div className="mx-2 my-1.5 h-px bg-border/70" />
                  <div className="px-2 pb-1 pt-0.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/80">
                    {t('sidebar.otherConnections')}
                  </div>
                </>
              )}

              {regularConnections.map(renderConnection)}
            </>
          )}
        </div>
      </section>

      <footer className="p-3 border-t border-border space-y-1">
        <Button
          className="w-full justify-start text-muted-foreground hover:text-foreground hover:bg-muted"
          variant="ghost"
          onClick={onNewConnection}
        >
          <Plus size={16} className="mr-2" />
          {t('sidebar.newConnection')}
        </Button>
        <Button
          className="w-full justify-start text-muted-foreground hover:text-foreground hover:bg-muted"
          variant="ghost"
          onClick={() => {
            AnalyticsService.capture('error_view_opened', { source: 'sidebar' });
            setLogsOpen(true);
          }}
        >
          <Bug size={16} className="mr-2" />
          {t('sidebar.errorLogs')}
        </Button>
      </footer>

      <ErrorLogPanel isOpen={logsOpen} onClose={() => setLogsOpen(false)} />
    </aside>
  );
}
