import { useState, useEffect } from 'react';
import { ConnectionItem } from './ConnectionItem';
import { DBTree } from '../Tree/DBTree';
import { ErrorLogPanel } from '../Logs/ErrorLogPanel';
import {
  listSavedConnections,
  connectSavedConnection,
  SavedConnection,
  Namespace,
} from '../../lib/tauri';
import { Plus, Bug } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { toast } from 'sonner';
import { useTranslation } from 'react-i18next';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import { APP_VERSION } from '@/lib/version';

const DEFAULT_PROJECT = 'default';

interface SidebarProps {
  onNewConnection: () => void;
  onConnected: (sessionId: string, connection: SavedConnection) => void;
  connectedSessionId: string | null;
  connectedConnectionId?: string | null;
  onTableSelect?: (namespace: Namespace, tableName: string) => void;
  onDatabaseSelect?: (namespace: Namespace) => void;
  onEditConnection: (connection: SavedConnection, password: string) => void;
  refreshTrigger?: number;
  schemaRefreshTrigger?: number;
}

export function Sidebar({
  onNewConnection,
  onConnected,
  connectedSessionId,
  connectedConnectionId,
  onTableSelect,
  onDatabaseSelect,
  onEditConnection,
  refreshTrigger,
  schemaRefreshTrigger,
}: SidebarProps) {
  const [connections, setConnections] = useState<SavedConnection[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [connecting, setConnecting] = useState<string | null>(null);
  const [logsOpen, setLogsOpen] = useState(false);

  const { t } = useTranslation();

  useEffect(() => {
    loadConnections();
  }, []);

  useEffect(() => {
    loadConnections();
  }, [connectedSessionId, refreshTrigger]);

  async function loadConnections() {
    try {
      const saved = await listSavedConnections(DEFAULT_PROJECT);
      setConnections(saved);
    } catch (err) {
      console.error('Failed to load connections:', err);
    }
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

  return (
    <aside className="w-64 h-full flex flex-col border-r border-border bg-muted/30">
      <header className="h-14 flex items-center justify-between px-4 border-b border-border">
        <button
          onClick={() => (window.location.href = '/')}
          className="flex items-center gap-2 font-semibold text-foreground"
        >
          <img src="/logo-white.png" alt="QoreDB" width={28} height={28} />
          QoreDB
        </button>
        <p className="text-xs text-muted-foreground">v{APP_VERSION}</p>
      </header>

      <section className="flex-1 overflow-auto py-2">
        <div className="px-3 mb-2 text-xs font-semibold text-muted-foreground uppercase tracking-wider">
          {t('sidebar.connections')}
        </div>
        <div className="px-2 space-y-0.5">
          {connections.length === 0 ? (
            <p className="px-2 py-4 text-sm text-center text-muted-foreground">
              {t('sidebar.noConnections')}
            </p>
          ) : (
            connections.map(conn => (
              <div key={conn.id}>
                <ConnectionItem
                  connection={conn}
                  isSelected={selectedId === conn.id}
                  isExpanded={expandedId === conn.id}
                  isConnected={connectedConnectionId === conn.id}
                  isConnecting={connecting === conn.id}
                  onSelect={() => handleSelect(conn)}
                  onEdit={onEditConnection}
                  onDeleted={loadConnections}
                />
                {expandedId === conn.id && connectedSessionId && (
                  <div className="pl-4 border-l border-border ml-4 mt-1">
                    <DBTree
                      connectionId={connectedSessionId}
                      driver={conn.driver}
                      connection={conn}
                      onTableSelect={onTableSelect}
                      onDatabaseSelect={onDatabaseSelect}
                      refreshTrigger={schemaRefreshTrigger}
                    />
                  </div>
                )}
              </div>
            ))
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
