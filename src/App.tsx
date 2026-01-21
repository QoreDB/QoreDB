import { useState, useEffect, useMemo, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { notify } from './lib/notify';
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
import { Tooltip } from './components/ui/tooltip';
import { Search, Settings, X } from 'lucide-react';
import {
  Namespace,
  SavedConnection,
  connectSavedConnection,
  listSavedConnections,
  getDriverInfo,
  DriverCapabilities,
} from './lib/tauri';
import { HistoryEntry } from './lib/history';
import { QueryLibraryItem } from './lib/queryLibrary';
import { Driver } from './lib/drivers';
import { OpenTab, createTableTab, createDatabaseTab, createQueryTab } from './lib/tabs';
import { Toaster } from 'sonner';
import { useTheme } from './hooks/useTheme';
import { QueryLibraryModal } from './components/Query/QueryLibraryModal';
import { OnboardingModal } from './components/Onboarding/OnboardingModal';
import { AnalyticsService } from './components/Onboarding/AnalyticsService';
import {
  CrashRecoverySnapshot,
  clearCrashRecoverySnapshot,
  getCrashRecoverySnapshot,
  saveCrashRecoverySnapshot,
} from './lib/crashRecovery';
import { check } from '@tauri-apps/plugin-updater';
import './index.css';

function isTextInputTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName.toLowerCase();
  return (
    tag === 'input' ||
    tag === 'textarea' ||
    tag === 'select' ||
    target.isContentEditable
  );
}

const DEFAULT_PROJECT = 'default';
const RECOVERY_SCRATCH_TAB_ID = 'scratch_query';
const RECOVERY_SAVE_DEBOUNCE_MS = 600;

function App() {
  const { t } = useTranslation();
  const { theme, toggleTheme } = useTheme();
  const [searchOpen, setSearchOpen] = useState(false);
  const [connectionModalOpen, setConnectionModalOpen] = useState(false);
  const [libraryModalOpen, setLibraryModalOpen] = useState(false);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [driver, setDriver] = useState<Driver>(Driver.Postgres);
  const [driverCapabilities, setDriverCapabilities] = useState<DriverCapabilities | null>(null);
  const [activeConnection, setActiveConnection] = useState<SavedConnection | null>(null);
  const [queryDrafts, setQueryDrafts] = useState<Record<string, string>>({});
  const [recoverySnapshot, setRecoverySnapshot] = useState<CrashRecoverySnapshot | null>(null);
  const [recoveryConnectionName, setRecoveryConnectionName] = useState<string | null>(null);
  const [recoveryMissing, setRecoveryMissing] = useState(false);
  const [recoveryLoading, setRecoveryLoading] = useState(false);
  const [recoveryError, setRecoveryError] = useState<string | null>(null);
  const [showOnboarding, setShowOnboarding] = useState(false);

  useEffect(() => {
    const handler = () => setSidebarRefreshTrigger(prev => prev + 1);
    window.addEventListener('qoredb:connections-changed', handler);
    return () => window.removeEventListener('qoredb:connections-changed', handler);
  }, []);

  useEffect(() => {
    if (!AnalyticsService.isOnboardingCompleted()) {
      setShowOnboarding(true);
    }
  }, []);

  useEffect(() => {
    if (!import.meta.env.PROD) return;

    let cancelled = false;
    const handle = window.setTimeout(async () => {
      try {
        const update = await check();
        if (!update || cancelled) return;

        notify.info(t('updates.available', { version: update.version }), {
          action: {
            label: t('updates.install'),
            onClick: async () => {
              try {
                notify.info(t('updates.installing'));
                await update.downloadAndInstall();
                notify.success(t('updates.installed'));
                notify.info(t('updates.restartRequired'));
              } catch (err) {
                notify.error(t('updates.installFailed'), err);
              }
            },
          },
        });
      } catch (err) {
        console.warn('Update check failed', err);
      }
    }, 4000);

    return () => {
      cancelled = true;
      window.clearTimeout(handle);
    };
  }, [t]);

  useEffect(() => {
    const snapshot = getCrashRecoverySnapshot();
    if (!snapshot) return;

    setRecoverySnapshot(snapshot);
    listSavedConnections(DEFAULT_PROJECT)
      .then(saved => {
        const match = saved.find(conn => conn.id === snapshot.connectionId);
        setRecoveryConnectionName(match?.name ?? null);
        setRecoveryMissing(!match);
      })
      .catch(() => {
        setRecoveryConnectionName(null);
        setRecoveryMissing(true);
      });
  }, []);

  useEffect(() => {
    if (!sessionId) {
      setDriverCapabilities(null);
      return;
    }

    let cancelled = false;
    getDriverInfo(sessionId)
      .then(response => {
        if (cancelled) return;
        if (response.success && response.driver) {
          setDriverCapabilities(response.driver.capabilities);
        } else {
          setDriverCapabilities(null);
        }
      })
      .catch(() => {
        if (!cancelled) setDriverCapabilities(null);
      });

    return () => {
      cancelled = true;
    };
  }, [sessionId]);

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

  const updateQueryDraft = useCallback((tabId: string, value: string) => {
    setQueryDrafts(prev => {
      if (prev[tabId] === value) return prev;
      return { ...prev, [tabId]: value };
    });
  }, []);

  function handleConnected(
    newSessionId: string,
    connection: SavedConnection,
    options?: {
      tabs?: OpenTab[];
      activeTabId?: string | null;
      queryDrafts?: Record<string, string>;
    }
  ) {
    setSessionId(newSessionId);
    setDriver(connection.driver as Driver);
    setActiveConnection(connection);
    setTabs(options?.tabs ?? []);
    setActiveTabId(
      options?.activeTabId ?? options?.tabs?.[0]?.id ?? null
    );
    setQueryDrafts(options?.queryDrafts ?? {});
    setSettingsOpen(false);
    setPendingQuery(undefined);
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
    if (tab.type === 'query' && tab.initialQuery) {
      setQueryDrafts(prev => (prev[tab.id] ? prev : { ...prev, [tab.id]: tab.initialQuery || '' }));
    }
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
      setQueryDrafts(prev => {
        if (!(tabId in prev)) return prev;
        const next = { ...prev };
        delete next[tabId];
        return next;
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
              notify.error(t('query.noConnectionError'));
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
          const connectResult = await connectSavedConnection(DEFAULT_PROJECT, conn.id);
          if (connectResult.success && connectResult.session_id) {
            notify.success(t('sidebar.connectedTo', { name: conn.name }));
            handleConnected(connectResult.session_id, {
              ...conn,
              environment: conn.environment,
              read_only: conn.read_only,
            });
            setSidebarRefreshTrigger(prev => prev + 1);
          } else {
            notify.error(t('sidebar.connectionToFailed', { name: conn.name }), connectResult.error);
          }
        } catch {
          notify.error(t('sidebar.connectError'));
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
    AnalyticsService.capture('resource_opened', {
      source: 'tree',
      resource_type: driver === Driver.Mongodb ? 'collection' : 'table',
      driver,
    });
    openTab(createTableTab(namespace, tableName));
  }

  function handleDatabaseSelect(namespace: Namespace) {
    setQueryNamespace(namespace);
    AnalyticsService.capture('resource_opened', {
      source: 'tree',
      resource_type: driver === Driver.Mongodb ? 'database' : 'schema',
      driver,
    });
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

  async function handleRestoreSession() {
    if (!recoverySnapshot) return;
    setRecoveryLoading(true);
    setRecoveryError(null);

    try {
      const saved = await listSavedConnections(DEFAULT_PROJECT);
      const match = saved.find(conn => conn.id === recoverySnapshot.connectionId);

      if (!match) {
        setRecoveryMissing(true);
        setRecoveryError(t('recovery.missingConnection'));
        return;
      }

      const result = await connectSavedConnection(DEFAULT_PROJECT, match.id);
      if (result.success && result.session_id) {
        const restoredTabs: OpenTab[] = recoverySnapshot.tabs.map(tab => {
          const restored: OpenTab = {
            id: tab.id,
            type: tab.type,
            title: tab.title,
            namespace: tab.namespace,
            tableName: tab.tableName,
          };

          if (tab.type === 'query') {
            const query = recoverySnapshot.queryDrafts[tab.id];
            if (query) {
              restored.initialQuery = query;
            }
          }

          return restored;
        });

        handleConnected(
          result.session_id,
          {
            ...match,
            environment: match.environment,
            read_only: match.read_only,
          },
          {
            tabs: restoredTabs,
            activeTabId: recoverySnapshot.activeTabId,
            queryDrafts: recoverySnapshot.queryDrafts,
          }
        );
        setRecoverySnapshot(null);
        setRecoveryMissing(false);
      } else {
        setRecoveryError(result.error || t('recovery.restoreFailed'));
      }
    } catch (err) {
      setRecoveryError(err instanceof Error ? err.message : t('common.unknownError'));
    } finally {
      setRecoveryLoading(false);
    }
  }

  const handleDiscardRecovery = useCallback(() => {
    clearCrashRecoverySnapshot();
    setRecoverySnapshot(null);
    setRecoveryConnectionName(null);
    setRecoveryMissing(false);
    setRecoveryError(null);
  }, []);

  useEffect(() => {
    if (!activeConnection || !sessionId) return;

    const snapshot: CrashRecoverySnapshot = {
      version: 1,
      updatedAt: Date.now(),
      projectId: DEFAULT_PROJECT,
      connectionId: activeConnection.id,
      activeTabId,
      tabs: tabs.map(tab => ({
        id: tab.id,
        type: tab.type,
        title: tab.title,
        namespace: tab.namespace,
        tableName: tab.tableName,
      })),
      queryDrafts,
    };

    const handle = window.setTimeout(() => {
      saveCrashRecoverySnapshot(snapshot);
    }, RECOVERY_SAVE_DEBOUNCE_MS);

    return () => window.clearTimeout(handle);
  }, [activeConnection, sessionId, tabs, activeTabId, queryDrafts]);

  // Global keyboard shortcuts
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const isOverlayOpen = searchOpen || connectionModalOpen || libraryModalOpen;
      if (isOverlayOpen) {
        if (e.key === 'Escape') {
          e.preventDefault();
          setSearchOpen(false);
          setConnectionModalOpen(false);
          setLibraryModalOpen(false);
        }
        return;
      }

      if (isTextInputTarget(e.target)) {
        if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
          e.preventDefault();
          setSearchOpen(true);
        }
        return;
      }

      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        setSearchOpen(true);
      }
      // Cmd+N: New connection
      if ((e.metaKey || e.ctrlKey) && e.key === 'n') {
        e.preventDefault();
        setConnectionModalOpen(true);
      }
      // Cmd+Shift+L: Open library
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'l') {
        e.preventDefault();
        setLibraryModalOpen(true);
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
  }, [
    activeTabId,
    closeTab,
    connectionModalOpen,
    handleNewQuery,
    libraryModalOpen,
    searchOpen,
    settingsOpen,
  ]);

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
            <Tooltip content={t('settings.title')} side="left">
              <Button
                variant="ghost"
                size="icon"
                onClick={() => setSettingsOpen(!settingsOpen)}
                className="text-muted-foreground hover:text-foreground transition-transform duration-150 active:scale-95"
                aria-label={t('settings.title')}
              >
                <span
                  className={`inline-flex transition-transform duration-200 ${
                    settingsOpen ? 'rotate-90 scale-110' : 'rotate-0 scale-100'
                  }`}
                >
                  {settingsOpen ? <X size={16} /> : <Settings size={16} />}
                </span>
              </Button>
            </Tooltip>
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
                  driverCapabilities={driverCapabilities}
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
                            driverCapabilities={driverCapabilities}
                            environment={activeConnection?.environment || 'development'}
                            readOnly={activeConnection?.read_only || false}
                            connectionName={activeConnection?.name}
                            connectionDatabase={activeConnection?.database}
                            activeNamespace={queryNamespace}
                            initialQuery={queryDrafts[tab.id] ?? tab.initialQuery}
                            onSchemaChange={triggerSchemaRefresh}
                            onOpenLibrary={() => setLibraryModalOpen(true)}
                            isActive={tab.id === activeTabId}
                            onQueryDraftChange={value => updateQueryDraft(tab.id, value)}
                          />
                        </div>
                      ))
                  ) : (
                    <div className="flex h-full w-full">
                      <QueryPanel
                        key={sessionId}
                        sessionId={sessionId}
                        dialect={driver}
                        driverCapabilities={driverCapabilities}
                        environment={activeConnection?.environment || 'development'}
                        readOnly={activeConnection?.read_only || false}
                        connectionName={activeConnection?.name}
                        connectionDatabase={activeConnection?.database}
                        activeNamespace={queryNamespace}
                        initialQuery={queryDrafts[RECOVERY_SCRATCH_TAB_ID] ?? pendingQuery}
                        onSchemaChange={triggerSchemaRefresh}
                        onOpenLibrary={() => setLibraryModalOpen(true)}
                        isActive
                        onQueryDraftChange={value =>
                          updateQueryDraft(RECOVERY_SCRATCH_TAB_ID, value)
                        }
                      />
                    </div>
                  )}
                </div>
              )
            ) : (
              <div className="flex flex-col items-center justify-center h-full text-center space-y-4">
                {recoverySnapshot && (
                  <div className="w-full max-w-xl text-left rounded-lg border border-border bg-muted/50 p-4 shadow-sm">
                    <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                      <div>
                        <p className="text-sm font-semibold text-foreground">
                          {t('recovery.title')}
                        </p>
                        <p className="text-sm text-muted-foreground">
                          {recoveryConnectionName
                            ? t('recovery.description', { name: recoveryConnectionName })
                            : t('recovery.descriptionUnknown')}
                        </p>
                        {recoveryMissing && (
                          <p className="text-xs text-error mt-2">
                            {t('recovery.missingConnection')}
                          </p>
                        )}
                        {recoveryError && !recoveryMissing && (
                          <p className="text-xs text-error mt-2">{recoveryError}</p>
                        )}
                      </div>
                      <div className="flex items-center gap-2">
                        <Button
                          variant="outline"
                          onClick={handleDiscardRecovery}
                          disabled={recoveryLoading}
                        >
                          {t('recovery.discard')}
                        </Button>
                        <Button
                          onClick={handleRestoreSession}
                          disabled={recoveryLoading || recoveryMissing}
                        >
                          {recoveryLoading ? t('recovery.restoring') : t('recovery.restore')}
                        </Button>
                      </div>
                    </div>
                  </div>
                )}
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

      {showOnboarding && <OnboardingModal onComplete={() => setShowOnboarding(false)} />}
    </>
  );
}

export default App;
