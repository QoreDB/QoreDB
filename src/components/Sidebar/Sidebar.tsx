import { useState, useEffect } from 'react';
import { ConnectionItem } from './ConnectionItem';
import { DBTree } from '../Tree/DBTree';
import { listSavedConnections, connect, getConnectionCredentials, SavedConnection, ConnectionConfig, Namespace } from '../../lib/tauri';
import { useTheme } from '../../hooks/useTheme';
import { Plus, Sun, Moon } from 'lucide-react';
import { Button } from '@/components/ui/button';

const DEFAULT_PROJECT = 'default';

interface SidebarProps {
  onNewConnection: () => void;
  onConnected: (sessionId: string, driver: string) => void;
  connectedSessionId: string | null;
  onTableSelect?: (namespace: Namespace, tableName: string) => void;
}

export function Sidebar({ onNewConnection, onConnected, connectedSessionId, onTableSelect }: SidebarProps) {
  const [connections, setConnections] = useState<SavedConnection[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [connecting, setConnecting] = useState<string | null>(null);
  const { isDark, toggleTheme } = useTheme();

  useEffect(() => {
    loadConnections();
  }, []);

  useEffect(() => {
    loadConnections();
  }, [connectedSessionId]);

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
        console.error('Failed to get credentials:', credsResult.error);
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
      };

      const result = await connect(config);
      
      if (result.success && result.session_id) {
        onConnected(result.session_id, conn.driver);
        setExpandedId(conn.id);
      } else {
        console.error('Connection failed:', result.error);
      }
    } catch (err) {
      console.error('Connection error:', err);
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
        <div className="flex items-center gap-2 font-semibold text-foreground">
          <img src="/logo.png" alt="QoreDB" width={24} height={24} />
          QoreDB
        </div>
        <Button
          variant="ghost" 
          size="icon"
          onClick={toggleTheme}
          title={isDark ? 'Switch to light mode' : 'Switch to dark mode'}
          className="h-8 w-8 text-muted-foreground hover:text-foreground"
        >
          {isDark ? <Sun size={16} /> : <Moon size={16} />}
        </Button>
      </header>

      <section className="flex-1 overflow-auto py-2">
        <div className="px-3 mb-2 text-xs font-semibold text-muted-foreground uppercase tracking-wider">
          Connections
        </div>
        <div className="px-2 space-y-0.5">
          {connections.length === 0 ? (
            <p className="px-2 py-4 text-sm text-center text-muted-foreground">
              No saved connections
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
                />
                {expandedId === conn.id && connectedSessionId && (
                  <div className="pl-4 border-l border-border ml-4 mt-1">
                    <DBTree 
                      connectionId={connectedSessionId} 
                      onTableSelect={onTableSelect}
                    />
                  </div>
                )}
              </div>
            ))
          )}
        </div>
      </section>

      <footer className="p-3 border-t border-border">
        <Button 
          className="w-full justify-start text-muted-foreground hover:text-foreground hover:bg-muted" 
          variant="ghost"
          onClick={onNewConnection}
        >
          <Plus size={16} className="mr-2" />
          New Connection
        </Button>
      </footer>
    </aside>
  );
}
