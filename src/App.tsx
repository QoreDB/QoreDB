import { useState, useEffect, useMemo, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Sidebar } from './components/Sidebar/Sidebar';
import { TabBar } from './components/Tabs/TabBar';
import { GlobalSearch, SearchResult } from './components/Search/GlobalSearch';
import { QueryPanel } from './components/Query/QueryPanel';
import { TableBrowser } from './components/Browser/TableBrowser';
import { DatabaseBrowser } from './components/Browser/DatabaseBrowser';
import { ConnectionModal } from './components/Connection/ConnectionModal';
import { SettingsPage } from './components/Settings/SettingsPage';
import { StatusBar } from './components/Status/StatusBar';
import { Button } from './components/ui/button';
import { Search, Settings, X } from 'lucide-react';
import { Namespace, SavedConnection, connectSavedConnection } from './lib/tauri';
import { HistoryEntry } from './lib/history';
import { QueryLibraryItem } from './lib/queryLibrary';
import { Driver } from './lib/drivers';
import { OpenTab, createTableTab, createDatabaseTab, createQueryTab } from './lib/tabs';
import { Toaster, toast } from 'sonner';
import { useTheme } from './hooks/useTheme';
import { QueryLibraryModal } from './components/Query/QueryLibraryModal';
import './index.css';

function App() {
  const { t } = useTranslation();
  const { theme, toggleTheme } = useTheme();
  const [searchOpen, setSearchOpen] = useState(false);
  const [connectionModalOpen, setConnectionModalOpen] = useState(false);
  const [libraryModalOpen, setLibraryModalOpen] = useState(false);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [driver, setDriver] = useState<Driver>(Driver.Postgres);
  const [activeConnection, setActiveConnection] = useState<SavedConnection | null>(null);

  // Tab system
  const [tabs, setTabs] = useState<OpenTab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);

  const [settingsOpen, setSettingsOpen] = useState(false);
  const [sidebarRefreshTrigger, setSidebarRefreshTrigger] = useState(0);
  const [schemaRefreshTrigger, setSchemaRefreshTrigger] = useState(0);
  const [queryNamespace, setQueryNamespace] = useState<Namespace | null>(null);

  // Edit connection state
  const [editConnection, setEditConnection] = useState<SavedConnection | null>(null);
  const [editPassword, setEditPassword] = useState<string>('');

  // Query injection from search
  const [pendingQuery, setPendingQuery] = useState<string | undefined>(undefined);

  function triggerSchemaRefresh() {
    setSchemaRefreshTrigger(prev => prev + 1);
  }

  function handleConnected(newSessionId: string, connection: SavedConnection) {
    setSessionId(newSessionId);
    setDriver(connection.driver as Driver);
    setActiveConnection(connection);
    setTabs([]);
    setActiveTabId(null);
    setSettingsOpen(false);
    setQueryNamespace(
      connection.database ? { database: connection.database } : null
    );
  }

  // Tab management
  const activeTab = useMemo(() => tabs.find(t => t.id === activeTabId), [tabs, activeTabId]);

  const openTab = useCallback((tab: OpenTab) => {
    setTabs(prev => {
      const existing =
        tab.type === 'query'
          ? undefined
          : prev.find(
              t =>
                t.type === tab.type &&
                t.namespace?.database === tab.namespace?.database &&
                t.namespace?.schema === tab.namespace?.schema &&
                t.tableName === tab.tableName
            );
      if (existing) {
        setActiveTabId(existing.id);
        return prev;
      }
      setActiveTabId(tab.id);
      return [...prev, tab];
    });
    setSettingsOpen(false);
  }, []);

  const closeTab = useCallback(
    (tabId: string) => {
      setTabs(prev => {
        const newTabs = prev.filter(t => t.id !== tabId);
        if (activeTabId === tabId) {
          const closedIndex = prev.findIndex(t => t.id === tabId);
          const newActiveTab = newTabs[closedIndex] || newTabs[closedIndex - 1] || null;
          setActiveTabId(newActiveTab?.id || null);
        }
        return newTabs;
      });
    },
    [activeTabId]
  );

  const paletteCommands = useMemo(
    () => [
      { id: 'cmd_new_connection', label: t('palette.newConnection'), shortcut: '⌘N' },
      { id: 'cmd_new_query', label: t('palette.newQuery'), shortcut: '⌘T' },
      { id: 'cmd_open_library', label: t('palette.openLibrary') },
      { id: 'cmd_open_settings', label: t('palette.openSettings'), shortcut: '⌘,' },
      { id: 'cmd_toggle_theme', label: t('palette.toggleTheme') },
      ...(activeTabId ? [{ id: 'cmd_close_tab', label: t('palette.closeTab'), shortcut: '⌘W' }] : []),
    ],
    [activeTabId, t]
  );

  // Handle search result selection
  const handleSearchSelect = useCallback(
    async (result: SearchResult) => {
      setSearchOpen(false);

      if (result.type === 'command') {
        switch (result.id) {
          case 'cmd_new_connection': {
            setConnectionModalOpen(true);
            return;
          }
          case 'cmd_new_query': {
            if (!sessionId) {
              toast.error(t('query.noConnectionError'));
              return;
            }
            openTab(createQueryTab());
            return;
          }
          case 'cmd_open_library': {
            setLibraryModalOpen(true);
            return;
          }
          case 'cmd_open_settings': {
            setSettingsOpen(true);
            return;
          }
          case 'cmd_toggle_theme': {
            toggleTheme();
            return;
          }
          case 'cmd_close_tab': {
            if (activeTabId) closeTab(activeTabId);
            return;
          }
          default:
            return;
        }
      }

      if (result.type === 'connection' && result.data) {
        // Connect to the selected connection
        const conn = result.data as SavedConnection;
        try {
          const connectResult = await connectSavedConnection('default', conn.id);
          if (connectResult.success && connectResult.session_id) {
            toast.success(t('sidebar.connectedTo', { name: conn.name }));
            handleConnected(connectResult.session_id, {
              ...conn,
              environment: conn.environment,
              read_only: conn.read_only,
            });
            setSidebarRefreshTrigger(prev => prev + 1);
          } else {
            toast.error(t('sidebar.connectionToFailed', { name: conn.name }), {
              description: connectResult.error,
            });
          }
        } catch {
          toast.error(t('sidebar.connectError'));
        }
      } else if (result.type === 'query' || result.type === 'favorite') {
        const entry = result.data as HistoryEntry;
        if (entry?.query) {
          if (sessionId) {
            openTab(createQueryTab(entry.query));
            setPendingQuery(undefined);
          } else {
            setPendingQuery(entry.query);
          }
          setSettingsOpen(false);
        }
      } else if (result.type === 'library') {
        const item = result.data as QueryLibraryItem;
        if (item?.query) {
          if (sessionId) {
            openTab(createQueryTab(item.query));
            setPendingQuery(undefined);
          } else {
            setPendingQuery(item.query);
          }
          setSettingsOpen(false);
        }
      }
    },
    [t, sessionId, openTab, toggleTheme, activeTabId, closeTab]
  );

  function handleTableSelect(namespace: Namespace, tableName: string) {
    setQueryNamespace(namespace);
    openTab(createTableTab(namespace, tableName));
  }

  function handleDatabaseSelect(namespace: Namespace) {
    setQueryNamespace(namespace);
    openTab(createDatabaseTab(namespace));
  }

  const handleNewQuery = useCallback(() => {
    if (sessionId) {
      openTab(createQueryTab());
    }
  }, [sessionId, openTab]);

  const handleEditConnection = useCallback((connection: SavedConnection, password: string) => {
    setEditConnection(connection);
    setEditPassword(password);
    setConnectionModalOpen(true);
  }, []);

  const handleCloseConnectionModal = useCallback(() => {
    setConnectionModalOpen(false);
    setEditConnection(null);
    setEditPassword('');
  }, []);

  const handleConnectionSaved = useCallback(
    (updatedConnection: SavedConnection) => {
      const isEditingActive = activeConnection?.id === updatedConnection.id;
      if (isEditingActive) {
        setActiveConnection(prev => (prev ? { ...prev, ...updatedConnection } : updatedConnection));
        setDriver(updatedConnection.driver as Driver);
      }

      handleCloseConnectionModal();
      setSidebarRefreshTrigger(prev => prev + 1);
    },
    [activeConnection?.id, handleCloseConnectionModal]
  );

  // Global keyboard shortcuts
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        setSearchOpen(true);
      }
      // Cmd+N: New connection
      if ((e.metaKey || e.ctrlKey) && e.key === 'n') {
        e.preventDefault();
        setConnectionModalOpen(true);
      }
      // Cmd+,: Settings
      if ((e.metaKey || e.ctrlKey) && e.key === ',') {
        e.preventDefault();
        setSettingsOpen(true);
      }
      // Escape: Close active tab or settings
      if (e.key === 'Escape') {
        if (activeTabId) {
          closeTab(activeTabId);
        } else if (settingsOpen) {
          setSettingsOpen(false);
        }
      }
      // Cmd+W: Close active tab
      if ((e.metaKey || e.ctrlKey) && e.key === 'w') {
        e.preventDefault();
        if (activeTabId) closeTab(activeTabId);
      }
      // Cmd+T: New query tab
      if ((e.metaKey || e.ctrlKey) && e.key === 't') {
        e.preventDefault();
        handleNewQuery();
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [activeTabId, settingsOpen, closeTab, handleNewQuery]);

  return (
    <>
      <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground font-sans">
        <Sidebar
          onNewConnection={() => setConnectionModalOpen(true)}
          onConnected={handleConnected}
          connectedSessionId={sessionId}
          connectedConnectionId={activeConnection?.id || null}
          onTableSelect={handleTableSelect}
          onDatabaseSelect={handleDatabaseSelect}
          onEditConnection={handleEditConnection}
          refreshTrigger={sidebarRefreshTrigger}
          schemaRefreshTrigger={schemaRefreshTrigger}
        />
        <main className="flex-1 flex flex-col min-w-0 min-h-0 bg-background relative">
          <header className="flex items-center justify-end absolute right-0 top-0 h-10 z-50 pr-2">
            <Button
              variant="ghost"
              size="icon"
              onClick={() => setSettingsOpen(!settingsOpen)}
              className="text-muted-foreground hover:text-foreground transition-transform duration-150 active:scale-95"
              title={t('settings.title')}
            >
              <span
                className={`inline-flex transition-transform duration-200 ${
                  settingsOpen ? 'rotate-90 scale-110' : 'rotate-0 scale-100'
                }`}
              >
                {settingsOpen ? <X size={16} /> : <Settings size={16} />}
              </span>
            </Button>
          </header>

          {!settingsOpen && sessionId && (
            <TabBar
              tabs={tabs.map(t => ({ id: t.id, title: t.title, type: t.type }))}
              activeId={activeTabId || undefined}
              onSelect={setActiveTabId}
              onClose={closeTab}
              onNew={handleNewQuery}
            />
          )}

          <div className="flex-1 min-h-0 overflow-hidden p-4 pt-12">
            {settingsOpen ? (
              <SettingsPage />
            ) : sessionId ? (
              activeTab?.type === 'table' && activeTab.namespace && activeTab.tableName ? (
                <TableBrowser
                  key={activeTab.id}
                  sessionId={sessionId}
                  namespace={activeTab.namespace}
                  tableName={activeTab.tableName}
                  driver={driver}
                  environment={activeConnection?.environment || 'development'}
                  readOnly={activeConnection?.read_only || false}
                  connectionName={activeConnection?.name}
                  connectionDatabase={activeConnection?.database}
                  onClose={() => closeTab(activeTab.id)}
                />
              ) : activeTab?.type === 'database' && activeTab.namespace ? (
                <DatabaseBrowser
                  key={activeTab.id}
                  sessionId={sessionId}
                  namespace={activeTab.namespace}
                  driver={driver}
                  environment={activeConnection?.environment || 'development'}
                  readOnly={activeConnection?.read_only || false}
                  connectionName={activeConnection?.name}
                  onTableSelect={handleTableSelect}
                  onSchemaChange={triggerSchemaRefresh}
                  onClose={() => closeTab(activeTab.id)}
                />
              ) : (
                <div className="flex-1 min-h-0">
                  {tabs.filter(tab => tab.type === 'query').length > 0 ? (
                    tabs
                      .filter(tab => tab.type === 'query')
                      .map(tab => (
                        <div
                          key={tab.id}
                          className={tab.id === activeTabId ? 'flex h-full w-full' : 'hidden'}
                        >
                          <QueryPanel
                            sessionId={sessionId}
                            dialect={driver}
                            environment={activeConnection?.environment || 'development'}
                            readOnly={activeConnection?.read_only || false}
                            connectionName={activeConnection?.name}
                            connectionDatabase={activeConnection?.database}
                            activeNamespace={queryNamespace}
                            initialQuery={tab.initialQuery}
                            onSchemaChange={triggerSchemaRefresh}
                            onOpenLibrary={() => setLibraryModalOpen(true)}
                          />
                        </div>
                      ))
                  ) : (
                    <QueryPanel
                      key={sessionId}
                      sessionId={sessionId}
                      dialect={driver}
                      environment={activeConnection?.environment || 'development'}
                      readOnly={activeConnection?.read_only || false}
                      connectionName={activeConnection?.name}
                      connectionDatabase={activeConnection?.database}
                      activeNamespace={queryNamespace}
                      initialQuery={pendingQuery}
                      onSchemaChange={triggerSchemaRefresh}
                      onOpenLibrary={() => setLibraryModalOpen(true)}
                    />
                  )}
                </div>
              )
            ) : (
              <div className="flex flex-col items-center justify-center h-full text-center space-y-4">
                <div className="p-4 rounded-full bg-accent/10 text-accent mb-4">
                  <img src="/logo.png" alt="QoreDB" width={48} height={48} />
                </div>
                <h2 className="text-2xl font-semibold tracking-tight">{t('app.welcome')}</h2>
                <p className="text-muted-foreground max-w-100">{t('app.description')}</p>
                <div className="flex flex-col gap-2 min-w-50">
                  <Button onClick={() => setConnectionModalOpen(true)} className="w-full">
                    + {t('app.newConnection')}
                  </Button>
                  <Button
                    variant="outline"
                    onClick={() => setSearchOpen(true)}
                    className="w-full text-muted-foreground"
                  >
                    <Search className="mr-2 h-4 w-4" />
                    {t('app.search')}{' '}
                    <kbd className="ml-auto pointer-events-none inline-flex h-5 select-none items-center gap-1 rounded border bg-muted px-1.5 font-mono text-[10px] font-medium text-muted-foreground opacity-100">
                      <span className="text-xs">⌘</span>K
                    </kbd>
                  </Button>
                </div>
              </div>
            )}
          </div>
          <StatusBar sessionId={sessionId} connection={activeConnection} />
        </main>
      </div>

      <ConnectionModal
        isOpen={connectionModalOpen}
        onClose={handleCloseConnectionModal}
        onConnected={handleConnected}
        editConnection={editConnection || undefined}
        editPassword={editPassword || undefined}
        onSaved={handleConnectionSaved}
      />

      <GlobalSearch
        isOpen={searchOpen}
        onClose={() => setSearchOpen(false)}
        onSelect={handleSearchSelect}
        commands={paletteCommands}
      />

      <QueryLibraryModal
        isOpen={libraryModalOpen}
        onClose={() => setLibraryModalOpen(false)}
        onSelectQuery={q => {
          if (sessionId) {
            openTab(createQueryTab(q));
            setPendingQuery(undefined);
          } else {
            setPendingQuery(q);
          }
        }}
      />

      <Toaster
        theme={theme === 'dark' ? 'dark' : 'light'}
        closeButton
        position="bottom-right"
        richColors
        toastOptions={{
          // className: "bg-background border-border text-foreground",
          duration: 4000,
        }}
      />
    </>
  );
}

export default App;
