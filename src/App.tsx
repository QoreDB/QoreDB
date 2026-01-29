import { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Toaster } from 'sonner';
import { check } from '@tauri-apps/plugin-updater';

// Components
import { CustomTitlebar } from './components/CustomTitlebar';
import { Sidebar } from './components/Sidebar/Sidebar';
import { TabBar } from './components/Tabs/TabBar';
import { StatusBar } from './components/Status/StatusBar';
import { SandboxBorder } from './components/Sandbox';
import { WelcomeScreen } from './components/Home/WelcomeScreen';
import { SettingsPage } from './components/Settings/SettingsPage';
import { QueryPanel } from './components/Query/QueryPanel';
import { TableBrowser, type TableBrowserTab } from './components/Browser/TableBrowser';
import { DatabaseBrowser, type DatabaseBrowserTab } from './components/Browser/DatabaseBrowser';
import { ConnectionDashboard } from './components/Dashboard/ConnectionDashboard';
import { ConnectionModal } from './components/Connection/ConnectionModal';
import { GlobalSearch, SearchResult } from './components/Search/GlobalSearch';
import { FulltextSearchPanel } from './components/Search/FulltextSearchPanel';
import { QueryLibraryModal } from './components/Query/QueryLibraryModal';
import { OnboardingModal } from './components/Onboarding/OnboardingModal';

// Hooks
import { useTheme } from './hooks/useTheme';
import { useTabs } from './hooks/useTabs';
import { useRecovery } from './hooks/useRecovery';
import { useKeyboardShortcuts } from './hooks/useKeyboardShortcuts';

// Lib
import { notify } from './lib/notify';
import {
  Namespace,
  SavedConnection,
  connectSavedConnection,
  listSavedConnections,
  getDriverInfo,
  DriverCapabilities,
  RelationFilter,
  SearchFilter,
} from './lib/tauri';
import { HistoryEntry } from './lib/history';
import { QueryLibraryItem } from './lib/queryLibrary';
import { Driver } from './lib/drivers';
import { OpenTab, createTableTab, createDatabaseTab, createQueryTab } from './lib/tabs';
import { CrashRecoverySnapshot, saveCrashRecoverySnapshot } from './lib/crashRecovery';
import { AnalyticsService } from './components/Onboarding/AnalyticsService';
import { getShortcut } from '@/utils/platform';
import {
  activateSandbox,
  deactivateSandbox,
  getSandboxPreferences,
  hasPendingChanges,
  isSandboxActive,
} from '@/lib/sandboxStore';
import {
  emitUiEvent,
  UI_EVENT_EXPORT_DATA,
  UI_EVENT_OPEN_HISTORY,
  UI_EVENT_OPEN_LOGS,
  UI_EVENT_REFRESH_TABLE,
} from '@/lib/uiEvents';

import './index.css';

// Constants
const DEFAULT_PROJECT = 'default';
const RECOVERY_SAVE_DEBOUNCE_MS = 600;

function App() {
  const { t } = useTranslation();
  const { resolvedTheme, toggleTheme } = useTheme();

  const [searchOpen, setSearchOpen] = useState(false);
  const [fulltextSearchOpen, setFulltextSearchOpen] = useState(false);
  const [connectionModalOpen, setConnectionModalOpen] = useState(false);
  const [libraryModalOpen, setLibraryModalOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [sidebarVisible, setSidebarVisible] = useState(true);

  const [sessionId, setSessionId] = useState<string | null>(null);
  const [driver, setDriver] = useState<Driver>(Driver.Postgres);
  const [driverCapabilities, setDriverCapabilities] = useState<DriverCapabilities | null>(null);
  const [activeConnection, setActiveConnection] = useState<SavedConnection | null>(null);
  const [hasConnections, setHasConnections] = useState(false);

  // Edit connection state
  const [editConnection, setEditConnection] = useState<SavedConnection | null>(null);
  const [editPassword, setEditPassword] = useState<string>('');

  const [sidebarRefreshTrigger, setSidebarRefreshTrigger] = useState(0);
  const [schemaRefreshTrigger, setSchemaRefreshTrigger] = useState(0);

  const {
    tabs,
    activeTabId,
    activeTab,
    queryDrafts,
    tableBrowserTabsRef,
    databaseBrowserTabsRef,
    openTab,
    closeTab,
    setActiveTabId,
    updateQueryDraft,
    reset: resetTabs,
  } = useTabs();

  const recovery = useRecovery();
  const recoverySaveHandleRef = useRef<number | null>(null);

  useEffect(() => {
    listSavedConnections(DEFAULT_PROJECT)
      .then(saved => setHasConnections(saved.length > 0))
      .catch(() => setHasConnections(false));
  }, [sidebarRefreshTrigger]);

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
    if (!sessionId) {
      setDriverCapabilities(null);
      return;
    }

    let cancelled = false;
    getDriverInfo(sessionId)
      .then(response => {
        if (cancelled) return;
        setDriverCapabilities(response.success && response.driver ? response.driver.capabilities : null);
      })
      .catch(() => {
        if (!cancelled) setDriverCapabilities(null);
      });

    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  const scheduleRecoverySave = useCallback(() => {
    if (!activeConnection || !sessionId) return;

    const snapshot: CrashRecoverySnapshot = {
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
      tableBrowserTabs: { ...tableBrowserTabsRef.current },
      databaseBrowserTabs: { ...databaseBrowserTabsRef.current },
    };

    if (recoverySaveHandleRef.current) {
      window.clearTimeout(recoverySaveHandleRef.current);
    }

    recoverySaveHandleRef.current = window.setTimeout(() => {
      saveCrashRecoverySnapshot(snapshot);
    }, RECOVERY_SAVE_DEBOUNCE_MS);
  }, [activeConnection, sessionId, tabs, activeTabId, queryDrafts, tableBrowserTabsRef, databaseBrowserTabsRef]);

  useEffect(() => {
    scheduleRecoverySave();
    return () => {
      if (recoverySaveHandleRef.current) {
        window.clearTimeout(recoverySaveHandleRef.current);
      }
    };
  }, [scheduleRecoverySave]);

  const handleConnected = useCallback(
    (
      newSessionId: string,
      connection: SavedConnection,
      options?: {
        tabs?: OpenTab[];
        activeTabId?: string | null;
        queryDrafts?: Record<string, string>;
        tableBrowserTabs?: Record<string, TableBrowserTab>;
        databaseBrowserTabs?: Record<string, DatabaseBrowserTab>;
      }
    ) => {
      setSessionId(newSessionId);
      setDriver(connection.driver as Driver);
      setActiveConnection(connection);
      setSettingsOpen(false);

      resetTabs({
        initialTabs: options?.tabs,
        initialActiveTabId: options?.activeTabId ?? options?.tabs?.[0]?.id ?? null,
        initialQueryDrafts: options?.queryDrafts,
        initialTableBrowserTabs: options?.tableBrowserTabs,
        initialDatabaseBrowserTabs: options?.databaseBrowserTabs,
      });
    },
    [resetTabs]
  );

  const handleRestoreSession = useCallback(async () => {
    const result = await recovery.restore();
    if (result) {
      notify.success(t('sidebar.connectedTo', { name: result.connection.name }));
      handleConnected(result.sessionId, result.connection, {
        tabs: result.tabs,
        activeTabId: result.activeTabId,
        queryDrafts: result.queryDrafts,
        tableBrowserTabs: result.tableBrowserTabs,
        databaseBrowserTabs: result.databaseBrowserTabs,
      });
      setSidebarRefreshTrigger(prev => prev + 1);
    }
  }, [recovery, handleConnected, t]);

  const handleTableSelect = useCallback(
    (namespace: Namespace, tableName: string, relationFilter?: RelationFilter, searchFilter?: SearchFilter) => {
      AnalyticsService.capture('resource_opened', {
        source: searchFilter ? 'search' : relationFilter ? 'relation' : 'tree',
        resource_type: driver === Driver.Mongodb ? 'collection' : 'table',
        driver,
      });
      openTab(createTableTab(namespace, tableName, relationFilter, searchFilter));
    },
    [driver, openTab]
  );

  const handleDatabaseSelect = useCallback(
    (namespace: Namespace) => {
      AnalyticsService.capture('resource_opened', {
        source: 'tree',
        resource_type: driver === Driver.Mongodb ? 'database' : 'schema',
        driver,
      });
      openTab(createDatabaseTab(namespace));
    },
    [driver, openTab]
  );

  const handleNewQuery = useCallback(() => {
    if (sessionId) {
      openTab(createQueryTab(undefined, activeTab?.namespace));
    }
  }, [sessionId, openTab, activeTab]);

  const handleToggleSidebar = useCallback(() => {
    setSidebarVisible(prev => !prev);
  }, []);

  const handleOpenLogs = useCallback(() => {
    emitUiEvent(UI_EVENT_OPEN_LOGS);
  }, []);

  const handleRefreshData = useCallback(() => {
    emitUiEvent(UI_EVENT_REFRESH_TABLE);
  }, []);

  const handleExportData = useCallback(() => {
    emitUiEvent(UI_EVENT_EXPORT_DATA, { format: 'csv' });
  }, []);

  const handleOpenHistory = useCallback(() => {
    if (!sessionId) {
      notify.error(t('query.noConnectionError'));
      return;
    }
    setSettingsOpen(false);
    if (activeTab?.type !== 'query') {
      openTab(createQueryTab(undefined, activeTab?.namespace));
      window.setTimeout(() => emitUiEvent(UI_EVENT_OPEN_HISTORY), 0);
      return;
    }
    emitUiEvent(UI_EVENT_OPEN_HISTORY);
  }, [activeTab?.namespace, activeTab?.type, openTab, sessionId, t]);

  const handleToggleSandbox = useCallback(() => {
    if (!sessionId) {
      notify.error(t('query.noConnectionError'));
      return;
    }

    const isActive = isSandboxActive(sessionId);
    if (isActive) {
      const prefs = getSandboxPreferences();
      if (prefs.confirmOnDiscard && hasPendingChanges(sessionId)) {
        const confirmExit = window.confirm(
          `${t('sandbox.confirmDeactivate.title')}\n\n${t('sandbox.confirmDeactivate.message')}`
        );
        if (!confirmExit) return;
        const discard = window.confirm(t('sandbox.confirmDeactivate.discardChanges'));
        deactivateSandbox(sessionId, discard);
        return;
      }
      deactivateSandbox(sessionId);
      return;
    }

    activateSandbox(sessionId);
    if (activeConnection?.environment === 'staging') {
      notify.warning(t('sandbox.envWarningStaging'));
    }
    if (activeConnection?.environment === 'production') {
      notify.warning(t('sandbox.envWarningProduction'));
    }
  }, [activeConnection?.environment, sessionId, t]);

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
      if (activeConnection?.id === updatedConnection.id) {
        setActiveConnection(prev => (prev ? { ...prev, ...updatedConnection } : updatedConnection));
        setDriver(updatedConnection.driver as Driver);
      }
      handleCloseConnectionModal();
      setSidebarRefreshTrigger(prev => prev + 1);
    },
    [activeConnection?.id, handleCloseConnectionModal]
  );

  const triggerSchemaRefresh = useCallback(() => {
    setSchemaRefreshTrigger(prev => prev + 1);
  }, []);

  const paletteCommands = useMemo(
    () => [
      { id: 'cmd_new_connection', label: t('palette.newConnection'), shortcut: getShortcut('N', { symbol: true }) },
      { id: 'cmd_new_query', label: t('palette.newQuery'), shortcut: getShortcut('T', { symbol: true }) },
      { id: 'cmd_open_library', label: t('palette.openLibrary') },
      ...(sessionId ? [{ id: 'cmd_fulltext_search', label: t('palette.fulltextSearch'), shortcut: getShortcut('F', { symbol: true, shift: true }) }] : []),
      { id: 'cmd_open_settings', label: t('palette.openSettings'), shortcut: getShortcut(',', { symbol: true }) },
      { id: 'cmd_toggle_theme', label: t('palette.toggleTheme') },
      ...(activeTabId ? [{ id: 'cmd_close_tab', label: t('palette.closeTab'), shortcut: getShortcut('W', { symbol: true }) }] : []),
    ],
    [activeTabId, sessionId, t]
  );

  const handleSearchSelect = useCallback(
    async (result: SearchResult) => {
      setSearchOpen(false);

      if (result.type === 'command') {
        switch (result.id) {
          case 'cmd_new_connection':
            setConnectionModalOpen(true);
            return;
          case 'cmd_new_query':
            if (!sessionId) {
              notify.error(t('query.noConnectionError'));
              return;
            }
            openTab(createQueryTab(undefined, activeTab?.namespace));
            return;
          case 'cmd_open_library':
            setLibraryModalOpen(true);
            return;
          case 'cmd_fulltext_search':
            if (sessionId) setFulltextSearchOpen(true);
            return;
          case 'cmd_open_settings':
            setSettingsOpen(true);
            return;
          case 'cmd_toggle_theme':
            toggleTheme();
            return;
          case 'cmd_close_tab':
            if (activeTabId) closeTab(activeTabId);
            return;
        }
      }

      if (result.type === 'connection' && result.data) {
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
        if (entry?.query && sessionId) {
          openTab(createQueryTab(entry.query));
          setSettingsOpen(false);
        }
      } else if (result.type === 'library') {
        const item = result.data as QueryLibraryItem;
        if (item?.query && sessionId) {
          openTab(createQueryTab(item.query));
          setSettingsOpen(false);
        }
      }
    },
    [t, sessionId, openTab, toggleTheme, activeTabId, closeTab, activeTab, handleConnected]
  );

  useKeyboardShortcuts({
    onSearch: () => setSearchOpen(true),
    onNewConnection: () => setConnectionModalOpen(true),
    onOpenLibrary: () => setLibraryModalOpen(true),
    onFulltextSearch: () => sessionId && setFulltextSearchOpen(true),
    onSettings: () => setSettingsOpen(true),
    onCloseTab: () => activeTabId && closeTab(activeTabId),
    onNewQuery: handleNewQuery,
    onEscape: () => {
      if (searchOpen) setSearchOpen(false);
      else if (fulltextSearchOpen) setFulltextSearchOpen(false);
      else if (connectionModalOpen) setConnectionModalOpen(false);
      else if (libraryModalOpen) setLibraryModalOpen(false);
      else if (activeTabId) closeTab(activeTabId);
      else if (settingsOpen) setSettingsOpen(false);
    },
    isOverlayOpen: searchOpen || fulltextSearchOpen || connectionModalOpen || libraryModalOpen,
    hasSession: Boolean(sessionId),
    hasActiveTab: Boolean(activeTabId),
  });

  const activeTableSubTab =
    activeTab?.type === 'table'
      ? (tableBrowserTabsRef.current[activeTab.id] ?? 'data')
      : null;
  const canRefreshData = Boolean(sessionId && activeTab?.type === 'table' && activeTableSubTab === 'data');
  const canExportData = Boolean(sessionId && activeTab?.type === 'table' && activeTableSubTab === 'data');
  const canOpenHistory = Boolean(sessionId);
  const canToggleSandbox = Boolean(sessionId);

  return (
    <>
      <div className="flex flex-col h-screen w-screen overflow-hidden bg-background text-foreground font-sans">
        <CustomTitlebar
          onOpenSearch={() => setSearchOpen(true)}
          onNewConnection={() => setConnectionModalOpen(true)}
          onOpenSettings={() => setSettingsOpen(!settingsOpen)}
          settingsOpen={settingsOpen}
          onOpenLogs={handleOpenLogs}
          onOpenHistory={canOpenHistory ? handleOpenHistory : undefined}
          onToggleSidebar={handleToggleSidebar}
          onRefreshData={canRefreshData ? handleRefreshData : undefined}
          onExportData={canExportData ? handleExportData : undefined}
          onToggleSandbox={canToggleSandbox ? handleToggleSandbox : undefined}
          readOnly={activeConnection?.read_only || false}
        />

        <div className="flex flex-1 overflow-hidden relative">
          {/* Settings overlay - full width */}
          {settingsOpen && (
            <div className="absolute inset-0 z-40 bg-background">
              <SettingsPage />
            </div>
          )}

          {/* Sidebar */}
          <div className={sidebarVisible ? '' : 'hidden'}>
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
              activeNamespace={activeTab?.namespace}
            />
          </div>

          <main className="flex-1 flex flex-col min-w-0 min-h-0 bg-background relative">
            <header className="flex items-center h-10 z-30 px-2 gap-2">
              <div className="flex items-center gap-2 flex-1 min-w-0">
                {!settingsOpen && sessionId && (
                  <TabBar
                    tabs={tabs.map(t => ({ id: t.id, title: t.title, type: t.type }))}
                    activeId={activeTabId || undefined}
                    onSelect={setActiveTabId}
                    onClose={closeTab}
                    onNew={handleNewQuery}
                  />
                )}
              </div>
            </header>

            {/* Content Area */}
            <SandboxBorder
              sessionId={sessionId}
              environment={activeConnection?.environment || 'development'}
              className="flex-1 min-h-0 overflow-hidden p-4"
            >
              {renderContent()}
            </SandboxBorder>

            <StatusBar sessionId={sessionId} connection={activeConnection} />
          </main>
        </div>
      </div>

      {/* Modals */}
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
          if (sessionId) openTab(createQueryTab(q));
        }}
      />

      <FulltextSearchPanel
        isOpen={fulltextSearchOpen}
        onClose={() => setFulltextSearchOpen(false)}
        sessionId={sessionId}
        onNavigateToTable={(ns, table, filter) => handleTableSelect(ns, table, undefined, filter)}
      />

      <Toaster
        theme={resolvedTheme}
        closeButton
        position="bottom-right"
        richColors
        toastOptions={{ duration: 4000 }}
      />

      {showOnboarding && <OnboardingModal onComplete={() => setShowOnboarding(false)} />}
    </>
  );

  function renderContent() {
    // No session: Welcome screen
    if (!sessionId) {
      return (
        <WelcomeScreen
          hasConnections={hasConnections}
          recovery={{
            snapshot: recovery.state.snapshot,
            connectionName: recovery.state.connectionName,
            isMissing: recovery.state.isMissing,
            isLoading: recovery.state.isLoading,
            error: recovery.state.error,
          }}
          onNewConnection={() => setConnectionModalOpen(true)}
          onRestoreSession={handleRestoreSession}
          onDiscardRecovery={recovery.discard}
          onOpenSearch={() => setSearchOpen(true)}
        />
      );
    }

    // Table browser
    if (activeTab?.type === 'table' && activeTab.namespace && activeTab.tableName) {
      return (
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
          connectionId={activeConnection?.id}
          onOpenRelatedTable={handleTableSelect}
          relationFilter={activeTab.relationFilter}
          searchFilter={activeTab.searchFilter}
          initialTab={tableBrowserTabsRef.current[activeTab.id]}
          onActiveTabChange={tab => {
            tableBrowserTabsRef.current[activeTab.id] = tab;
            scheduleRecoverySave();
          }}
          onClose={() => closeTab(activeTab.id)}
        />
      );
    }

    // Database browser
    if (activeTab?.type === 'database' && activeTab.namespace) {
      return (
        <DatabaseBrowser
          key={activeTab.id}
          sessionId={sessionId}
          namespace={activeTab.namespace}
          driver={driver}
          environment={activeConnection?.environment || 'development'}
          readOnly={activeConnection?.read_only || false}
          connectionName={activeConnection?.name}
          onTableSelect={handleTableSelect}
          schemaRefreshTrigger={schemaRefreshTrigger}
          onSchemaChange={triggerSchemaRefresh}
          initialTab={databaseBrowserTabsRef.current[activeTab.id]}
          onActiveTabChange={tab => {
            databaseBrowserTabsRef.current[activeTab.id] = tab;
            scheduleRecoverySave();
          }}
          onOpenQueryTab={ns => openTab(createQueryTab(undefined, ns))}
          onOpenFulltextSearch={() => setFulltextSearchOpen(true)}
          onClose={() => closeTab(activeTab.id)}
        />
      );
    }

    // Query panel
    if (activeTab?.type === 'query') {
      return (
        <div className="flex-1 min-h-0">
          <QueryPanel
            key={activeTab.id}
            sessionId={sessionId}
            dialect={driver}
            driverCapabilities={driverCapabilities}
            environment={activeConnection?.environment || 'development'}
            readOnly={activeConnection?.read_only || false}
            connectionName={activeConnection?.name}
            connectionDatabase={activeConnection?.database}
            activeNamespace={activeTab.namespace}
            initialQuery={queryDrafts[activeTab.id] ?? activeTab.initialQuery}
            onSchemaChange={triggerSchemaRefresh}
            onOpenLibrary={() => setLibraryModalOpen(true)}
            isActive
            onQueryDraftChange={value => updateQueryDraft(activeTab.id, value)}
          />
        </div>
      );
    }

    // Connection dashboard (no tab selected)
    if (activeConnection) {
      return (
        <ConnectionDashboard
          sessionId={sessionId}
          driver={driver}
          connection={activeConnection}
          schemaRefreshTrigger={schemaRefreshTrigger}
          onSchemaChange={triggerSchemaRefresh}
          onOpenQuery={handleNewQuery}
          onOpenDatabase={handleDatabaseSelect}
        />
      );
    }

    return null;
  }
}

export default App;
