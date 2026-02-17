// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useMemo } from 'react';
import { ConnectionItem } from './ConnectionItem';
import { DBTree } from '../Tree/DBTree';
import { ErrorLogPanel } from '../Logs/ErrorLogPanel';
import {
  listSavedConnections,
  connectSavedConnection,
  SavedConnection,
  Namespace,
  RelationFilter,
  Collection,
} from '../../lib/tauri';
import { Plus, Bug } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { toast } from 'sonner';
import { useTranslation } from 'react-i18next';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import { useTheme } from '@/hooks/useTheme';
import { useLicense } from '@/providers/LicenseProvider';
import { LicenseBadge } from '@/components/License/LicenseBadge';
import { UI_EVENT_OPEN_LOGS } from '@/lib/uiEvents';
import {
  reconcileFavoriteConnectionIds,
  saveFavoriteConnectionIds,
} from '@/lib/connectionFavorites';

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
  onEditConnection: (connection: SavedConnection, password: string) => void;
  refreshTrigger?: number;
  schemaRefreshTrigger?: number;
  activeNamespace?: Namespace | null;
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
  onEditConnection,
  refreshTrigger,
  schemaRefreshTrigger,
  activeNamespace,
}: SidebarProps) {
  const [connections, setConnections] = useState<SavedConnection[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [connecting, setConnecting] = useState<string | null>(null);
  const [logsOpen, setLogsOpen] = useState(false);
  const [favoriteConnectionIds, setFavoriteConnectionIds] = useState<string[]>([]);

  const { t } = useTranslation();
  const { resolvedTheme } = useTheme();
  const { tier } = useLicense();

  useEffect(() => {
    loadConnections();
  }, []);

  useEffect(() => {
    loadConnections();
  }, [connectedSessionId, refreshTrigger]);

  useEffect(() => {
    if (connectedConnectionId) {
      setSelectedId(connectedConnectionId);
      setExpandedId(connectedConnectionId);
    }
  }, [connectedConnectionId]);

  useEffect(() => {
    const handler = () => setLogsOpen(true);
    window.addEventListener(UI_EVENT_OPEN_LOGS, handler);
    return () => window.removeEventListener(UI_EVENT_OPEN_LOGS, handler);
  }, []);

  const favoriteConnectionSet = useMemo(
    () => new Set(favoriteConnectionIds),
    [favoriteConnectionIds]
  );

  const connectionsById = useMemo(
    () => new Map(connections.map(connection => [connection.id, connection])),
    [connections]
  );

  const favoriteConnections = useMemo(
    () =>
      favoriteConnectionIds
        .map(connectionId => connectionsById.get(connectionId))
        .filter((connection): connection is SavedConnection => Boolean(connection)),
    [favoriteConnectionIds, connectionsById]
  );

  const regularConnections = useMemo(
    () => connections.filter(connection => !favoriteConnectionSet.has(connection.id)),
    [connections, favoriteConnectionSet]
  );

  async function loadConnections() {
    try {
      const saved = await listSavedConnections(DEFAULT_PROJECT);
      setConnections(saved);
      setFavoriteConnectionIds(
        reconcileFavoriteConnectionIds(saved.map(connection => connection.id))
      );
    } catch (err) {
      console.error('Failed to load connections:', err);
    }
  }

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
        />
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
      className="w-64 h-full flex flex-col border-r border-border bg-muted/30"
      data-allow-webview-shortcuts
    >
      <header className="h-12 flex items-center px-4 border-b border-border bg-muted/10">
        <button
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

      <section className="flex-1 overflow-auto py-2">
        <div className="px-2 space-y-0.5 mt-1">
          {connections.length === 0 ? (
            <p className="px-2 py-4 text-sm text-center text-muted-foreground">
              {t('sidebar.noConnections')}
            </p>
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
