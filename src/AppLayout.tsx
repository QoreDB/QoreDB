// SPDX-License-Identifier: Apache-2.0

import { useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Toaster } from 'sonner';
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
import { getShortcut } from '@/utils/platform';
import { DatabaseBrowser, type DatabaseBrowserTab } from './components/Browser/DatabaseBrowser';
import { TableBrowser, type TableBrowserTab } from './components/Browser/TableBrowser';
import { ConnectionModal } from './components/Connection/ConnectionModal';
import { CustomTitlebar } from './components/CustomTitlebar';
import { ConnectionDashboard } from './components/Dashboard/ConnectionDashboard';
import { DataDiffViewer } from './components/Diff/DataDiffViewer';
import { FederationViewer } from './components/Federation/FederationViewer';
import { WelcomeScreen } from './components/Home/WelcomeScreen';
import { LicenseGate } from './components/License/LicenseGate';
import { AnalyticsService } from './components/Onboarding/AnalyticsService';
import { OnboardingModal } from './components/Onboarding/OnboardingModal';
import { QueryLibraryModal } from './components/Query/QueryLibraryModal';
import { QueryPanel } from './components/Query/QueryPanel';
import { SandboxBorder } from './components/Sandbox';
import { FulltextSearchPanel } from './components/Search/FulltextSearchPanel';
import { GlobalSearch, type SearchResult } from './components/Search/GlobalSearch';
import { SettingsPage } from './components/Settings/SettingsPage';
import { Sidebar } from './components/Sidebar/Sidebar';
import { StatusBar } from './components/Status/StatusBar';
import { TabBar } from './components/Tabs/TabBar';
import type { useRecovery } from './hooks/useRecovery';
import { useTheme } from './hooks/useTheme';
import { useWebviewGuards } from './hooks/useWebviewGuards';
import { Driver } from './lib/drivers';
import type { HistoryEntry } from './lib/history';
import { notify } from './lib/notify';
import type { QueryLibraryItem } from './lib/queryLibrary';
import {
  createDatabaseTab,
  createDiffTab,
  createFederationTab,
  createQueryTab,
  createTableTab,
  type OpenTab,
} from './lib/tabs';
import {
  type Collection,
  connectSavedConnection,
  type DriverCapabilities,
  type Namespace,
  type RelationFilter,
  type SavedConnection,
  type SearchFilter,
} from './lib/tauri';
import { useModalContext } from './providers/ModalProvider';
import { useSessionContext } from './providers/SessionProvider';
import { useTabContext } from './providers/TabProvider';

const DEFAULT_PROJECT = 'default';

export function AppLayout() {
  const { t } = useTranslation();
  const { resolvedTheme, toggleTheme } = useTheme();
  useWebviewGuards();

  const {
    tabs,
    activeTabId,
    activeTab,
    queryDrafts,
    tableBrowserTabs,
    databaseBrowserTabs,
    openTab,
    closeTab,
    setActiveTabId,
    updateQueryDraft,
    updateTabNamespace,
    updateTableBrowserTab,
    updateDatabaseBrowserTab,
  } = useTabContext();

  const {
    sessionId,
    driver,
    driverCapabilities,
    activeConnection,
    hasConnections,
    sidebarRefreshTrigger,
    schemaRefreshTrigger,
    recovery,
    handleConnected,
    handleRestoreSession,
    handleConnectionSaved,
    refreshSidebar,
    triggerSchemaRefresh,
    scheduleRecoverySave,
  } = useSessionContext();

  const {
    searchOpen,
    fulltextSearchOpen,
    connectionModalOpen,
    libraryModalOpen,
    settingsOpen,
    sidebarVisible,
    showOnboarding,
    editConnection,
    editPassword,
    setSearchOpen,
    setFulltextSearchOpen,
    setConnectionModalOpen,
    setLibraryModalOpen,
    setSettingsOpen,
    setShowOnboarding,
    handleEditConnection,
    handleCloseConnectionModal,
    toggleSidebar,
  } = useModalContext();

  // --- Action handlers ---

  const handleTableSelect = useCallback(
    (ns: Namespace, tableName: string, rf?: RelationFilter, sf?: SearchFilter) => {
      AnalyticsService.capture('resource_opened', {
        source: sf ? 'search' : rf ? 'relation' : 'tree',
        resource_type: driver === Driver.Mongodb ? 'collection' : 'table',
        driver,
      });
      openTab(createTableTab(ns, tableName, rf, sf));
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
    if (sessionId) openTab(createQueryTab(undefined, activeTab?.namespace));
  }, [sessionId, openTab, activeTab?.namespace]);

  const handleOpenDiff = useCallback(() => {
    if (sessionId)
      openTab(createDiffTab(undefined, undefined, t('diff.title'), activeTab?.namespace));
  }, [sessionId, openTab, t, activeTab?.namespace]);

  const handleCompareTable = useCallback(
    (collection: Collection) => {
      if (!sessionId) return;
      const leftSource = {
        type: 'table' as const,
        label: collection.name,
        namespace: collection.namespace,
        tableName: collection.name,
        connectionId: activeConnection?.id,
      };
      openTab(
        createDiffTab(
          leftSource,
          undefined,
          `${t('diff.title')}: ${collection.name}`,
          collection.namespace
        )
      );
    },
    [sessionId, openTab, t, activeConnection?.id]
  );

  const handleAiGenerateForTable = useCallback(
    (collection: Collection) => {
      if (!sessionId) return;
      const tab = createQueryTab(undefined, collection.namespace);
      tab.showAiPanel = true;
      tab.aiTableContext = collection.name;
      openTab(tab);
    },
    [sessionId, openTab]
  );

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
  }, [activeTab?.namespace, activeTab?.type, openTab, sessionId, t, setSettingsOpen]);

  const handleToggleSandbox = useCallback(() => {
    if (!sessionId) {
      notify.error(t('query.noConnectionError'));
      return;
    }
    // Sandbox is available in Core with a 3-change limit — no full block here
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
    if (activeConnection?.environment === 'staging') notify.warning(t('sandbox.envWarningStaging'));
    if (activeConnection?.environment === 'production')
      notify.warning(t('sandbox.envWarningProduction'));
  }, [activeConnection?.environment, sessionId, t]);

  // --- Palette ---

  const paletteCommands = useMemo(
    () => [
      {
        id: 'cmd_new_connection',
        label: t('palette.newConnection'),
        shortcut: getShortcut('N', { symbol: true }),
      },
      {
        id: 'cmd_new_query',
        label: t('palette.newQuery'),
        shortcut: getShortcut('T', { symbol: true }),
      },
      { id: 'cmd_open_library', label: t('palette.openLibrary') },
      ...(sessionId
        ? [
            {
              id: 'cmd_fulltext_search',
              label: t('palette.fulltextSearch'),
              shortcut: getShortcut('F', { symbol: true, shift: true }),
            },
          ]
        : []),
      ...(sessionId ? [{ id: 'cmd_open_diff', label: t('diff.openDiff') }] : []),
      ...(sessionId
        ? [{ id: 'cmd_open_federation', label: t('federation.openFederation') }]
        : []),
      {
        id: 'cmd_open_settings',
        label: t('palette.openSettings'),
        shortcut: getShortcut(',', { symbol: true }),
      },
      { id: 'cmd_toggle_theme', label: t('palette.toggleTheme') },
      ...(activeTabId
        ? [
            {
              id: 'cmd_close_tab',
              label: t('palette.closeTab'),
              shortcut: getShortcut('W', { symbol: true }),
            },
          ]
        : []),
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
          case 'cmd_open_diff':
            if (sessionId) handleOpenDiff();
            return;
          case 'cmd_open_federation':
            if (sessionId) openTab(createFederationTab());
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
          const r = await connectSavedConnection(DEFAULT_PROJECT, conn.id);
          if (r.success && r.session_id) {
            notify.success(t('sidebar.connectedTo', { name: conn.name }));
            handleConnected(r.session_id, {
              ...conn,
              environment: conn.environment,
              read_only: conn.read_only,
            });
            refreshSidebar();
          } else {
            notify.error(t('sidebar.connectionToFailed', { name: conn.name }), r.error);
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
    [
      t,
      sessionId,
      openTab,
      toggleTheme,
      activeTabId,
      closeTab,
      activeTab?.namespace,
      handleConnected,
      handleOpenDiff,
      setSearchOpen,
      setConnectionModalOpen,
      setLibraryModalOpen,
      setFulltextSearchOpen,
      setSettingsOpen,
      refreshSidebar,
    ]
  );

  // --- Derived state ---
  const canRefreshData = Boolean(sessionId && activeTab?.type === 'table');
  const canExportData = Boolean(sessionId && activeTab?.type === 'table');

  return (
    <>
      <div className="flex flex-col h-screen w-screen overflow-hidden bg-background text-foreground font-sans">
        <CustomTitlebar
          onOpenSearch={() => setSearchOpen(true)}
          onNewConnection={() => setConnectionModalOpen(true)}
          onOpenSettings={() => setSettingsOpen(!settingsOpen)}
          settingsOpen={settingsOpen}
          onOpenLogs={() => emitUiEvent(UI_EVENT_OPEN_LOGS)}
          onOpenHistory={sessionId ? handleOpenHistory : undefined}
          onToggleSidebar={toggleSidebar}
          onRefreshData={canRefreshData ? () => emitUiEvent(UI_EVENT_REFRESH_TABLE) : undefined}
          onExportData={
            canExportData ? () => emitUiEvent(UI_EVENT_EXPORT_DATA, { format: 'csv' }) : undefined
          }
          onToggleSandbox={sessionId ? handleToggleSandbox : undefined}
          readOnly={activeConnection?.read_only || false}
        />

        <div className="flex flex-1 overflow-hidden relative">
          {settingsOpen && (
            <div className="absolute inset-0 z-40 bg-background animate-in fade-in slide-in-from-right-2 duration-200">
              <SettingsPage onClose={() => setSettingsOpen(false)} />
            </div>
          )}

          <div className={sidebarVisible ? '' : 'hidden'}>
            <Sidebar
              onNewConnection={() => setConnectionModalOpen(true)}
              onConnected={handleConnected}
              connectedSessionId={sessionId}
              connectedConnectionId={activeConnection?.id || null}
              onTableSelect={handleTableSelect}
              onDatabaseSelect={handleDatabaseSelect}
              onCompareTable={handleCompareTable}
              onAiGenerateForTable={handleAiGenerateForTable}
              onEditConnection={handleEditConnection}
              onNewQuery={handleNewQuery}
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

            <SandboxBorder
              sessionId={sessionId}
              environment={activeConnection?.environment || 'development'}
              className="flex-1 min-h-0 overflow-hidden p-4"
            >
              <AppContent
                sessionId={sessionId}
                driver={driver}
                driverCapabilities={driverCapabilities}
                activeConnection={activeConnection}
                activeTab={activeTab}
                queryDrafts={queryDrafts}
                tableBrowserTabs={tableBrowserTabs}
                databaseBrowserTabs={databaseBrowserTabs}
                onUpdateTableBrowserTab={updateTableBrowserTab}
                onUpdateDatabaseBrowserTab={updateDatabaseBrowserTab}
                hasConnections={hasConnections}
                recovery={recovery}
                schemaRefreshTrigger={schemaRefreshTrigger}
                onTableSelect={handleTableSelect}
                onDatabaseSelect={handleDatabaseSelect}
                onNewQuery={handleNewQuery}
                onOpenLibrary={() => setLibraryModalOpen(true)}
                onOpenFulltextSearch={() => setFulltextSearchOpen(true)}
                onRestoreSession={handleRestoreSession}
                onOpenSearch={() => setSearchOpen(true)}
                onOpenConnectionModal={() => setConnectionModalOpen(true)}
                onSchemaChange={triggerSchemaRefresh}
                onCloseTab={closeTab}
                onOpenTab={openTab}
                onUpdateQueryDraft={updateQueryDraft}
                onUpdateTabNamespace={updateTabNamespace}
                onScheduleRecoverySave={scheduleRecoverySave}
              />
            </SandboxBorder>

            <StatusBar sessionId={sessionId} connection={activeConnection} />
          </main>
        </div>
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
}

// --- AppContent: main content area based on active tab ---

interface AppContentProps {
  sessionId: string | null;
  driver: Driver;
  driverCapabilities: DriverCapabilities | null;
  activeConnection: SavedConnection | null;
  activeTab: OpenTab | undefined;
  queryDrafts: Record<string, string>;
  tableBrowserTabs: Record<string, TableBrowserTab>;
  databaseBrowserTabs: Record<string, DatabaseBrowserTab>;
  hasConnections: boolean;
  recovery: ReturnType<typeof useRecovery>;
  schemaRefreshTrigger: number;
  onTableSelect: (ns: Namespace, table: string, rf?: RelationFilter, sf?: SearchFilter) => void;
  onDatabaseSelect: (ns: Namespace) => void;
  onNewQuery: () => void;
  onOpenLibrary: () => void;
  onOpenFulltextSearch: () => void;
  onRestoreSession: () => Promise<void>;
  onOpenSearch: () => void;
  onOpenConnectionModal: () => void;
  onSchemaChange: () => void;
  onCloseTab: (id: string) => void;
  onOpenTab: (tab: OpenTab) => void;
  onUpdateQueryDraft: (tabId: string, value: string) => void;
  onUpdateTabNamespace: (tabId: string, namespace: Namespace) => void;
  onUpdateTableBrowserTab: (tabId: string, tab: TableBrowserTab) => void;
  onUpdateDatabaseBrowserTab: (tabId: string, tab: DatabaseBrowserTab) => void;
  onScheduleRecoverySave: () => void;
}

function AppContent({
  sessionId,
  driver,
  driverCapabilities,
  activeConnection,
  activeTab,
  queryDrafts,
  tableBrowserTabs,
  databaseBrowserTabs,
  hasConnections,
  recovery,
  schemaRefreshTrigger,
  onTableSelect,
  onDatabaseSelect,
  onNewQuery,
  onOpenLibrary,
  onOpenFulltextSearch,
  onRestoreSession,
  onOpenSearch,
  onOpenConnectionModal,
  onSchemaChange,
  onCloseTab,
  onOpenTab,
  onUpdateQueryDraft,
  onUpdateTabNamespace,
  onUpdateTableBrowserTab,
  onUpdateDatabaseBrowserTab,
  onScheduleRecoverySave,
}: AppContentProps) {
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
        onNewConnection={onOpenConnectionModal}
        onRestoreSession={onRestoreSession}
        onDiscardRecovery={recovery.discard}
        onOpenSearch={onOpenSearch}
      />
    );
  }

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
        onOpenRelatedTable={onTableSelect}
        relationFilter={activeTab.relationFilter}
        searchFilter={activeTab.searchFilter}
        initialTab={tableBrowserTabs[activeTab.id]}
        onActiveTabChange={tab => {
          onUpdateTableBrowserTab(activeTab.id, tab);
          onScheduleRecoverySave();
        }}
        onClose={() => onCloseTab(activeTab.id)}
      />
    );
  }

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
        connectionId={activeConnection?.id}
        onTableSelect={onTableSelect}
        schemaRefreshTrigger={schemaRefreshTrigger}
        onSchemaChange={onSchemaChange}
        initialTab={databaseBrowserTabs[activeTab.id]}
        onActiveTabChange={tab => {
          onUpdateDatabaseBrowserTab(activeTab.id, tab);
          onScheduleRecoverySave();
        }}
        onOpenQueryTab={ns => onOpenTab(createQueryTab(undefined, ns))}
        onOpenFulltextSearch={onOpenFulltextSearch}
        onClose={() => onCloseTab(activeTab.id)}
      />
    );
  }

  if (activeTab?.type === 'federation') {
    return (
      <div className="flex-1 min-h-0 flex flex-col">
        <FederationViewer
          key={activeTab.id}
          initialQuery={queryDrafts[activeTab.id] ?? activeTab.initialQuery}
        />
      </div>
    );
  }

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
          onSchemaChange={onSchemaChange}
          onOpenLibrary={onOpenLibrary}
          onNamespaceChange={ns => onUpdateTabNamespace(activeTab.id, ns)}
          isActive
          onQueryDraftChange={value => onUpdateQueryDraft(activeTab.id, value)}
          initialShowAiPanel={activeTab.showAiPanel}
          aiTableContext={activeTab.aiTableContext}
        />
      </div>
    );
  }

  if (activeTab?.type === 'diff') {
    return (
      <div className="flex-1 min-h-0 flex flex-col">
        <LicenseGate feature="visual_diff">
          <DataDiffViewer
            key={activeTab.id}
            activeConnection={activeConnection}
            namespace={activeTab.namespace}
            leftSource={activeTab.diffLeftSource}
            rightSource={activeTab.diffRightSource}
          />
        </LicenseGate>
      </div>
    );
  }

  if (activeConnection) {
    return (
      <ConnectionDashboard
        sessionId={sessionId}
        driver={driver}
        connection={activeConnection}
        schemaRefreshTrigger={schemaRefreshTrigger}
        onSchemaChange={onSchemaChange}
        onOpenQuery={onNewQuery}
        onOpenDatabase={onDatabaseSelect}
      />
    );
  }

  return null;
}
