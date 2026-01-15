import { useState, useEffect } from 'react';
import { ConnectionItem } from './ConnectionItem';
import { DBTree } from '../Tree/DBTree';
import { ErrorLogPanel } from '../Logs/ErrorLogPanel';
import { listSavedConnections, connect, getConnectionCredentials, SavedConnection, ConnectionConfig, Namespace } from '../../lib/tauri';
import { Plus, Bug } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { toast } from 'sonner';
import { useTranslation } from 'react-i18next'

const DEFAULT_PROJECT = 'default';

interface SidebarProps {
  onNewConnection: () => void;
  onConnected: (sessionId: string, connection: SavedConnection) => void;
  connectedSessionId: string | null;
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
      const credsResult = await getConnectionCredentials('default', conn.id);
      
      if (!credsResult.success || !credsResult.password) {
        toast.error(t('sidebar.failedToGetCredentials'), {
          description: credsResult.error || t('sidebar.couldNotRetrievePassword'),
        });
        return;
      }

      const config: ConnectionConfig = {
        driver: conn.driver,
        host: conn.host,
        port: conn.port,
        username: conn.username,
        password: credsResult.password,
        database: conn.database,
        ssl: conn.ssl,
        read_only: conn.read_only,
      };

      const result = await connect(config);
      
      if (result.success && result.session_id) {
        toast.success(t('sidebar.connectedTo', { name: conn.name }));
        onConnected(result.session_id, {
          ...conn,
          environment: conn.environment || 'development',
          read_only: conn.read_only || false,
        });
        setExpandedId(conn.id);
      } else {
        toast.error(t('sidebar.connectionToFailed', { name: conn.name }), {
          description: result.error || t('common.unknownError'),
        });
      }
    } catch (err) {
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
          onClick={() => window.location.href = '/'}
           className="flex items-center gap-2 font-semibold text-foreground">
          <img src="/logo.png" alt="QoreDB" width={24} height={24} />
          QoreDB
        </button>
        <p className="text-xs text-muted-foreground">
          v1.0.0
        </p>
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
                  isConnected={connectedSessionId !== null && selectedId === conn.id}
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
          onClick={() => setLogsOpen(true)}
        >
          <Bug size={16} className="mr-2" />
          {t('sidebar.errorLogs')}
        </Button>
      </footer>

      <ErrorLogPanel isOpen={logsOpen} onClose={() => setLogsOpen(false)} />
    </aside>
  );
}
