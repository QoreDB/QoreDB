import { useState, useEffect, useMemo } from 'react';
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
import { Search, Settings } from 'lucide-react';
import { Namespace, SavedConnection, connect, getConnectionCredentials, ConnectionConfig } from './lib/tauri';
import { HistoryEntry } from './lib/history';
import { Driver } from './lib/drivers';
import { OpenTab, createTableTab, createDatabaseTab, createQueryTab } from './lib/tabs';
import { Toaster, toast } from 'sonner';
import { useTheme } from './hooks/useTheme';
import './index.css';

function App() {
  const { t } = useTranslation();
  const { theme } = useTheme();
  const [searchOpen, setSearchOpen] = useState(false);
  const [connectionModalOpen, setConnectionModalOpen] = useState(false);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [driver, setDriver] = useState<Driver>('postgres');
  const [activeConnection, setActiveConnection] = useState<SavedConnection | null>(null);
  
  // Tab system
  const [tabs, setTabs] = useState<OpenTab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [sidebarRefreshTrigger, setSidebarRefreshTrigger] = useState(0);
  const [schemaRefreshTrigger, setSchemaRefreshTrigger] = useState(0);
  
  // Edit connection state
  const [editConnection, setEditConnection] = useState<SavedConnection | null>(null);
  const [editPassword, setEditPassword] = useState<string>('');
  
  // Query injection from search
  const [pendingQuery, setPendingQuery] = useState<string | undefined>(undefined);

  function triggerSchemaRefresh() {
    setSchemaRefreshTrigger(prev => prev + 1);
  }

  // Handle search result selection
  async function handleSearchSelect(result: SearchResult) {
    setSearchOpen(false);
    
    if (result.type === 'connection' && result.data) {
      // Connect to the selected connection
      const conn = result.data as SavedConnection;
      try {
        const credsResult = await getConnectionCredentials('default', conn.id);
        if (!credsResult.success || !credsResult.password) {
          toast.error(t('sidebar.failedToGetCredentials'));
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
        
        const connectResult = await connect(config);
        if (connectResult.success && connectResult.session_id) {
          toast.success(t('sidebar.connectedTo', { name: conn.name }));
          handleConnected(connectResult.session_id, {
            ...conn,
            environment: conn.environment || 'development',
            read_only: conn.read_only || false,
          });
          setSidebarRefreshTrigger(prev => prev + 1);
        } else {
          toast.error(t('sidebar.connectionToFailed', { name: conn.name }), {
            description: connectResult.error,
          });
        }
      } catch (err) {
        toast.error(t('sidebar.connectError'));
      }
    } else if (result.type === 'query' || result.type === 'favorite') {
      // Inject the query into the editor
      const entry = result.data as HistoryEntry;
      if (entry?.query) {
        setPendingQuery(entry.query);
        // Clear tabs to show query panel
        setActiveTabId(null);
        setSettingsOpen(false);
      }
    }
  }

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

  function handleConnected(newSessionId: string, connection: SavedConnection) {
    setSessionId(newSessionId);
    setDriver(connection.driver as Driver);
    setActiveConnection(connection);
    setTabs([]);
    setActiveTabId(null);
    setSettingsOpen(false);
  }

  // Tab management
  const activeTab = useMemo(() => tabs.find(t => t.id === activeTabId), [tabs, activeTabId]);

  function openTab(tab: OpenTab) {
    // Check if already open
    const existing = tabs.find(t => 
      t.type === tab.type && 
      t.namespace?.database === tab.namespace?.database &&
      t.namespace?.schema === tab.namespace?.schema &&
      t.tableName === tab.tableName
    );
    if (existing) {
      setActiveTabId(existing.id);
    } else {
      setTabs(prev => [...prev, tab]);
      setActiveTabId(tab.id);
    }
    setSettingsOpen(false);
  }

  function closeTab(tabId: string) {
    setTabs(prev => {
      const newTabs = prev.filter(t => t.id !== tabId);
      if (activeTabId === tabId) {
        // Activate previous or next tab
        const closedIndex = prev.findIndex(t => t.id === tabId);
        const newActiveTab = newTabs[closedIndex] || newTabs[closedIndex - 1] || null;
        setActiveTabId(newActiveTab?.id || null);
      }
      return newTabs;
    });
  }

  function handleTableSelect(namespace: Namespace, tableName: string) {
    openTab(createTableTab(namespace, tableName));
  }

  function handleDatabaseSelect(namespace: Namespace) {
    openTab(createDatabaseTab(namespace));
  }

  function handleNewQuery() {
    if (sessionId) {
      openTab(createQueryTab());
    }
  }

  function handleEditConnection(connection: SavedConnection, password: string) {
    setEditConnection(connection);
    setEditPassword(password);
    setConnectionModalOpen(true);
  }

  function handleConnectionSaved(updatedConnection: SavedConnection) {
			const isEditingActive = activeConnection?.id === updatedConnection.id;
			if (isEditingActive) {
				setActiveConnection((prev) =>
					prev ? { ...prev, ...updatedConnection } : updatedConnection
				);
				setDriver(updatedConnection.driver as Driver);
			}

			handleCloseConnectionModal();
			setSidebarRefreshTrigger((prev) => prev + 1);
		}

  function handleCloseConnectionModal() {
    setConnectionModalOpen(false);
    setEditConnection(null);
    setEditPassword('');
  }

  return (
			<>
				<div className="flex h-screen w-screen overflow-hidden bg-background text-foreground font-sans">
					<Sidebar
						onNewConnection={() => setConnectionModalOpen(true)}
						onConnected={handleConnected}
						connectedSessionId={sessionId}
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
								className="text-muted-foreground hover:text-foreground"
								title={t("settings.title")}
							>
								<Settings size={20} />
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

          <div className="flex-1 min-h-0 overflow-auto p-4 pt-12">
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
									environment={activeConnection?.environment || "development"}
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
									environment={activeConnection?.environment || "development"}
									readOnly={activeConnection?.read_only || false}
									connectionName={activeConnection?.name}
									onTableSelect={handleTableSelect}
									onSchemaChange={triggerSchemaRefresh}
									onClose={() => closeTab(activeTab.id)}
								/>
							) : (
								<QueryPanel
									key={activeTab?.id || sessionId}
									sessionId={sessionId}
									dialect={driver}
									environment={activeConnection?.environment || "development"}
									readOnly={activeConnection?.read_only || false}
									connectionName={activeConnection?.name}
									connectionDatabase={activeConnection?.database}
									initialQuery={pendingQuery}
								/>
							)
							) : (
								<div className="flex flex-col items-center justify-center h-full text-center space-y-4">
									<div className="p-4 rounded-full bg-accent/10 text-accent mb-4">
										<img src="/logo.png" alt="QoreDB" width={48} height={48} />
									</div>
									<h2 className="text-2xl font-semibold tracking-tight">
										{t("app.welcome")}
									</h2>
									<p className="text-muted-foreground max-w-100">
										{t("app.description")}
									</p>
									<div className="flex flex-col gap-2 min-w-50">
										<Button
											onClick={() => setConnectionModalOpen(true)}
											className="w-full"
										>
											+ {t("app.newConnection")}
										</Button>
										<Button
											variant="outline"
											onClick={() => setSearchOpen(true)}
											className="w-full text-muted-foreground"
										>
											<Search className="mr-2 h-4 w-4" />
											{t("app.search")}{" "}
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
				/>

				<Toaster
					theme={theme === "dark" ? "dark" : "light"}
					closeButton
					position="bottom-right"
					toastOptions={{
						className: "bg-background border-border text-foreground",
						duration: 4000,
					}}
				/>
			</>
		);
}

export default App;


