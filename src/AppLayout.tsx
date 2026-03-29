// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo } from 'react';
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
import { AppOverlays } from './components/AppOverlays';
import { DatabaseBrowser, type DatabaseBrowserTab } from './components/Browser/DatabaseBrowser';
import { TableBrowser, type TableBrowserTab } from './components/Browser/TableBrowser';
import { CustomTitlebar } from './components/CustomTitlebar';
import { ConnectionDashboard } from './components/Dashboard/ConnectionDashboard';
import { DataDiffViewer } from './components/Diff/DataDiffViewer';
import { FederationViewer } from './components/Federation/FederationViewer';
import { WelcomeScreen } from './components/Home/WelcomeScreen';
import { LicenseGate } from './components/License/LicenseGate';
import { NotebookTab } from './components/Notebook';
import { AnalyticsService } from './components/Onboarding/AnalyticsService';
import { QueryPanel } from './components/Query/QueryPanel';
import { SandboxBorder } from './components/Sandbox';
import type { SearchResult } from './components/Search/GlobalSearch';
import { SettingsPage } from './components/Settings/SettingsPage';
import { Sidebar } from './components/Sidebar/Sidebar';
import { SnapshotManager } from './components/Snapshot/SnapshotManager';
import { StatusBar } from './components/Status/StatusBar';
import { TabBar } from './components/Tabs/TabBar';
import { FeatureTour } from './components/Tour/FeatureTour';
import { ErrorBoundary } from './components/ui/error-boundary';
import { SkipLink } from './components/ui/skip-link';
import type { useRecovery } from './hooks/useRecovery';
import { useResizableSidebar } from './hooks/useResizableSidebar';
import { useTheme } from './hooks/useTheme';
import { useTourManager } from './hooks/useTourManager';
import { useWebviewGuards } from './hooks/useWebviewGuards';
import { Driver } from './lib/drivers';
import type { HistoryEntry } from './lib/history';
import {
  handleEditConnection,
  setConnectionModalOpen,
  setFulltextSearchOpen,
  setLibraryModalOpen,
  setSearchOpen,
  setSettingsOpen,
  toggleSidebar,
  toggleZenMode,
  useModalStore,
} from './lib/modalStore';
import { openNotebookFromFile, setPendingNotebook } from './lib/notebookIO';
import { notify } from './lib/notify';
import type { QueryLibraryItem } from './lib/queryLibrary';
import { getRoutineTemplate } from './lib/routineTemplates';
import {
  createDatabaseTab,
  createDiffTab,
  createFederationTab,
  createNotebookTab,
  createQueryTab,
  createSnapshotsTab,
  createTableTab,
  type OpenTab,
} from './lib/tabs';
import {
  type Collection,
  connectSavedConnection,
  type DatabaseEvent,
  type DriverCapabilities,
  getEventDefinition,
  getRoutineDefinition,
  getSequenceDefinition,
  getTriggerDefinition,
  type Namespace,
  type RelationFilter,
  type Routine,
  type RoutineType,
  type SavedConnection,
  type SearchFilter,
  type Sequence,
  type Trigger,
} from './lib/tauri';
import { getEventTemplate, getTriggerTemplate } from './lib/triggerTemplates';
import { useSessionContext } from './providers/SessionProvider';
import { useTabContext } from './providers/TabProvider';

const DEFAULT_PROJECT = 'default';

export function AppLayout() {
  const { t } = useTranslation();
  const { resolvedTheme, toggleTheme } = useTheme();
  useWebviewGuards();
  const { width: sidebarWidth, handleMouseDown: handleSidebarResizeStart } = useResizableSidebar();
  const tourManager = useTourManager();

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
    updateTab,
    reorderTabs,
    togglePinTab,
    setBeforeCloseTab,
  } = useTabContext();

  const {
    sessionId,
    driver,
    driverCapabilities,
    activeConnection,
    connectionHealth,
    hasConnections,
    schemaRefreshTrigger,
    recovery,
    handleConnected,
    handleRestoreSession,
    handleConnectionSaved,
    refreshSidebar,
    triggerSchemaRefresh,
    scheduleRecoverySave,
  } = useSessionContext();

  const settingsOpen = useModalStore(s => s.settingsOpen);
  const sidebarVisible = useModalStore(s => s.sidebarVisible);
  const zenMode = useModalStore(s => s.zenMode);

  // --- Zen mode toast ---
  useEffect(() => {
    if (zenMode) {
      notify.info(t('zenMode.enabled'), { duration: 2500 });
    }
  }, [zenMode, t]);

  // --- Notebook unsaved changes guard ---
  useEffect(() => {
    setBeforeCloseTab((tabId: string) => {
      const tab = tabs.find(t => t.id === tabId);
      if (tab?.type === 'notebook' && tab.notebookDirty) {
        return window.confirm(t('notebook.unsavedChanges'));
      }
      return true;
    });
  }, [tabs, setBeforeCloseTab, t]);

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

  const handleNewNotebook = useCallback(() => {
    if (sessionId) openTab(createNotebookTab());
  }, [sessionId, openTab]);

  const handleOpenNotebook = useCallback(async () => {
    if (!sessionId) return;
    try {
      const nbResult = await openNotebookFromFile();
      if (nbResult) {
        setPendingNotebook(nbResult.path, nbResult.notebook);
        openTab(createNotebookTab(nbResult.notebook.metadata.title, nbResult.path));
      }
    } catch {
      /* dialog cancelled or invalid file */
    }
  }, [sessionId, openTab]);

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

  const handleOpenRoutineSource = useCallback(
    async (routine: Routine, namespace: Namespace) => {
      if (!sessionId) return;
      const result = await getRoutineDefinition(
        sessionId,
        namespace.database,
        namespace.schema,
        routine.name,
        routine.routine_type,
        routine.arguments || undefined
      );
      if (result.success && result.definition) {
        const tab = createQueryTab(result.definition.definition, namespace);
        tab.title = `${routine.routine_type === 'Function' ? 'fn' : 'proc'}: ${routine.name}`;
        openTab(tab);
      } else {
        notify.error(t('routineManager.sourceLoadError'), result.error);
      }
    },
    [sessionId, openTab, t]
  );

  const handleCreateRoutine = useCallback(
    (routineType: RoutineType, namespace: Namespace) => {
      if (!sessionId) return;
      const template = getRoutineTemplate(driver as Driver, routineType, namespace);
      const tab = createQueryTab(template, namespace);
      tab.title =
        routineType === 'Function'
          ? t('routineManager.createFunction')
          : t('routineManager.createProcedure');
      openTab(tab);
    },
    [sessionId, driver, openTab, t]
  );

  const handleOpenTriggerSource = useCallback(
    async (trigger: Trigger, namespace: Namespace) => {
      if (!sessionId) return;
      const result = await getTriggerDefinition(
        sessionId,
        namespace.database,
        namespace.schema,
        trigger.name
      );
      if (result.success && result.definition) {
        const tab = createQueryTab(result.definition.definition, namespace);
        tab.title = `trigger: ${trigger.name}`;
        openTab(tab);
      } else {
        notify.error(t('triggerManager.sourceLoadError'), result.error);
      }
    },
    [sessionId, openTab, t]
  );

  const handleCreateTrigger = useCallback(
    (namespace: Namespace) => {
      if (!sessionId) return;
      const template = getTriggerTemplate(driver as Driver, namespace);
      const tab = createQueryTab(template, namespace);
      tab.title = t('triggerManager.createTrigger');
      openTab(tab);
    },
    [sessionId, driver, openTab, t]
  );

  const handleOpenEventSource = useCallback(
    async (event: DatabaseEvent, namespace: Namespace) => {
      if (!sessionId) return;
      const result = await getEventDefinition(
        sessionId,
        namespace.database,
        namespace.schema,
        event.name
      );
      if (result.success && result.definition) {
        const tab = createQueryTab(result.definition.definition, namespace);
        tab.title = `event: ${event.name}`;
        openTab(tab);
      } else {
        notify.error(t('eventManager.sourceLoadError'), result.error);
      }
    },
    [sessionId, openTab, t]
  );

  const handleOpenSequenceSource = useCallback(
    async (sequence: Sequence, namespace: Namespace) => {
      if (!sessionId) return;
      const result = await getSequenceDefinition(
        sessionId,
        namespace.database,
        namespace.schema,
        sequence.name
      );
      if (result.success && result.definition) {
        const tab = createQueryTab(result.definition.definition, namespace);
        tab.title = `seq: ${sequence.name}`;
        openTab(tab);
      } else {
        notify.error(t('sequenceManager.sourceLoadError'), result.error);
      }
    },
    [sessionId, openTab, t]
  );

  const handleCreateEvent = useCallback(
    (namespace: Namespace) => {
      if (!sessionId) return;
      const template = getEventTemplate(namespace);
      const tab = createQueryTab(template, namespace);
      tab.title = t('eventManager.createEvent');
      openTab(tab);
    },
    [sessionId, openTab, t]
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
  }, [activeTab?.namespace, activeTab?.type, openTab, sessionId, t]);

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

  const paletteFeatures = useMemo(
    () =>
      sessionId
        ? [
            {
              id: 'feat_notebook',
              label: t('features.notebooks.name'),
              sublabel: t('features.notebooks.description'),
            },
            {
              id: 'feat_sandbox',
              label: t('features.sandbox.name'),
              sublabel: t('features.sandbox.description'),
            },
            {
              id: 'feat_federation',
              label: t('features.federation.name'),
              sublabel: t('features.federation.description'),
            },
            {
              id: 'feat_diff',
              label: t('features.diff.name'),
              sublabel: t('features.diff.description'),
            },
            {
              id: 'feat_snapshots',
              label: t('features.snapshots.name'),
              sublabel: t('features.snapshots.description'),
            },
            {
              id: 'feat_fulltext',
              label: t('features.fulltextSearch.name'),
              sublabel: t('features.fulltextSearch.description'),
            },
            {
              id: 'feat_ai',
              label: t('features.aiAssistant.name'),
              sublabel: t('features.aiAssistant.description'),
            },
            {
              id: 'feat_er',
              label: t('features.erDiagram.name'),
              sublabel: t('features.erDiagram.description'),
            },
            {
              id: 'feat_virtual_relations',
              label: t('features.virtualRelations.name'),
              sublabel: t('features.virtualRelations.description'),
            },
          ]
        : [],
    [sessionId, t]
  );

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
      ...(sessionId ? [{ id: 'cmd_open_federation', label: t('federation.openFederation') }] : []),
      ...(sessionId ? [{ id: 'cmd_new_notebook', label: t('palette.newNotebook') }] : []),
      ...(sessionId ? [{ id: 'cmd_open_notebook', label: t('palette.openNotebook') }] : []),
      ...(sessionId && activeTab?.type === 'query'
        ? [{ id: 'cmd_convert_to_notebook', label: t('palette.convertToNotebook') }]
        : []),
      { id: 'cmd_open_snapshots', label: t('snapshots.openManager') },
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
    [activeTabId, activeTab?.type, sessionId, t]
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
          case 'cmd_open_snapshots':
            openTab(createSnapshotsTab());
            return;
          case 'cmd_open_federation':
            if (sessionId) openTab(createFederationTab());
            return;
          case 'cmd_new_notebook':
            if (sessionId) openTab(createNotebookTab());
            return;
          case 'cmd_open_notebook':
            if (sessionId) {
              try {
                const nbResult = await openNotebookFromFile();
                if (nbResult) {
                  setPendingNotebook(nbResult.path, nbResult.notebook);
                  openTab(createNotebookTab(nbResult.notebook.metadata.title, nbResult.path));
                }
              } catch {
                /* dialog cancelled or invalid file */
              }
            }
            return;
          case 'cmd_convert_to_notebook':
            if (sessionId && activeTab?.type === 'query') {
              const draft = queryDrafts[activeTab.id] ?? '';
              openTab(createNotebookTab(undefined, undefined, draft));
            }
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
      } else if (result.type === 'feature') {
        switch (result.id) {
          case 'feat_notebook':
            if (sessionId) openTab(createNotebookTab());
            return;
          case 'feat_sandbox':
            if (sessionId) handleToggleSandbox();
            return;
          case 'feat_federation':
            if (sessionId) openTab(createFederationTab());
            return;
          case 'feat_diff':
            if (sessionId) handleOpenDiff();
            return;
          case 'feat_snapshots':
            openTab(createSnapshotsTab());
            return;
          case 'feat_fulltext':
            if (sessionId) setFulltextSearchOpen(true);
            return;
          case 'feat_ai':
            setSettingsOpen(true);
            return;
          case 'feat_er':
          case 'feat_virtual_relations':
            // These features are accessed via table context menu / schema browser
            return;
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
      activeTab?.type,
      activeTab?.id,
      queryDrafts,
      handleConnected,
      handleOpenDiff,
      handleToggleSandbox,
      refreshSidebar,
    ]
  );

  // --- Derived state ---
  const canRefreshData = Boolean(sessionId && activeTab?.type === 'table');
  const canExportData = Boolean(sessionId && activeTab?.type === 'table');

  return (
    <>
      <div className="flex flex-col h-screen w-screen overflow-hidden bg-background text-foreground font-sans">
        <SkipLink />
        {!zenMode && (
          <CustomTitlebar
            onOpenSearch={() => setSearchOpen(true)}
            onNewConnection={() => setConnectionModalOpen(true)}
            onOpenNotebook={sessionId ? handleOpenNotebook : undefined}
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
            onToggleZenMode={toggleZenMode}
            readOnly={activeConnection?.read_only || false}
          />
        )}

        <div className="flex flex-1 overflow-hidden relative">
          {settingsOpen && (
            <div className="absolute inset-0 z-40 bg-background animate-in fade-in slide-in-from-right-2 duration-200">
              <SettingsPage onClose={() => setSettingsOpen(false)} />
            </div>
          )}

          {!zenMode && sidebarVisible && (
            <aside aria-label={t('a11y.sidebar')}>
              <Sidebar
                onNewConnection={() => setConnectionModalOpen(true)}
                onConnected={handleConnected}
                connectedSessionId={sessionId}
                connectedConnectionId={activeConnection?.id || null}
                onTableSelect={handleTableSelect}
                onDatabaseSelect={handleDatabaseSelect}
                onCompareTable={handleCompareTable}
                onAiGenerateForTable={handleAiGenerateForTable}
                onOpenRoutineSource={handleOpenRoutineSource}
                onCreateRoutine={handleCreateRoutine}
                onOpenTriggerSource={handleOpenTriggerSource}
                onCreateTrigger={handleCreateTrigger}
                onOpenEventSource={handleOpenEventSource}
                onCreateEvent={handleCreateEvent}
                onOpenSequenceSource={handleOpenSequenceSource}
                onEditConnection={handleEditConnection}
                onNewQuery={handleNewQuery}
                onNewNotebook={handleNewNotebook}
                schemaRefreshTrigger={schemaRefreshTrigger}
                activeNamespace={activeTab?.namespace}
                style={{ width: sidebarWidth, minWidth: sidebarWidth }}
              />
              <button
                type="button"
                aria-label="Resize sidebar"
                onMouseDown={handleSidebarResizeStart}
                className="w-1 shrink-0 cursor-col-resize bg-transparent hover:bg-accent/50 active:bg-accent transition-colors border-0 p-0 outline-none"
              />
            </aside>
          )}

          <main
            id="main-content"
            className="flex-1 flex flex-col min-w-0 min-h-0 bg-background relative"
          >
            {!zenMode && (
              <header className="flex items-center h-10 z-30 px-2 gap-2">
                <div className="flex items-center gap-2 flex-1 min-w-0">
                  {!settingsOpen && sessionId && (
                    <TabBar
                      tabs={tabs.map(t => ({
                        id: t.id,
                        title: t.title,
                        type: t.type,
                        pinned: t.pinned,
                      }))}
                      activeId={activeTabId || undefined}
                      onSelect={setActiveTabId}
                      onClose={closeTab}
                      onNew={handleNewQuery}
                      onReorder={reordered =>
                        reorderTabs(
                          reordered.flatMap(t => {
                            const full = tabs.find(f => f.id === t.id);
                            return full ? [full] : [];
                          })
                        )
                      }
                      onTogglePin={togglePinTab}
                    />
                  )}
                </div>
              </header>
            )}

            <SandboxBorder
              sessionId={sessionId}
              environment={activeConnection?.environment || 'development'}
              className={`flex-1 min-h-0 overflow-hidden ${zenMode ? '' : 'p-4'}`}
            >
              <ErrorBoundary fallbackLabel={t('errorBoundary.panelCrashed')}>
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
                  onUpdateTab={updateTab}
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
                  onOpenRoutineSource={handleOpenRoutineSource}
                  onCreateRoutine={handleCreateRoutine}
                  onOpenTriggerSource={handleOpenTriggerSource}
                  onCreateTrigger={handleCreateTrigger}
                  onOpenEventSource={handleOpenEventSource}
                  onCreateEvent={handleCreateEvent}
                  onOpenSequenceSource={handleOpenSequenceSource}
                />
              </ErrorBoundary>
            </SandboxBorder>

            {!zenMode && (
              <StatusBar
                sessionId={sessionId}
                connection={activeConnection}
                connectionHealth={connectionHealth}
              />
            )}
          </main>
        </div>
      </div>

      <AppOverlays
        onConnected={handleConnected}
        onConnectionSaved={handleConnectionSaved}
        onSearchSelect={handleSearchSelect}
        onSelectLibraryQuery={query => {
          if (sessionId) openTab(createQueryTab(query));
        }}
        onNavigateToTable={(ns, table, filter) => handleTableSelect(ns, table, undefined, filter)}
        paletteCommands={paletteCommands}
        paletteFeatures={paletteFeatures}
        sessionId={sessionId}
      />
      <Toaster
        theme={resolvedTheme}
        closeButton
        position="bottom-right"
        richColors
        toastOptions={{ duration: 4000 }}
      />
      {tourManager.activeTour && tourManager.activeTourSteps && (
        <FeatureTour
          steps={tourManager.activeTourSteps}
          onComplete={() => {
            if (tourManager.activeTour) tourManager.completeTour(tourManager.activeTour);
          }}
          onDismiss={() => tourManager.dismissTour()}
        />
      )}
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
  onUpdateTab: (tabId: string, updates: Partial<OpenTab>) => void;
  onScheduleRecoverySave: () => void;
  onOpenRoutineSource: (routine: Routine, namespace: Namespace) => void;
  onCreateRoutine: (routineType: RoutineType, namespace: Namespace) => void;
  onOpenTriggerSource: (trigger: Trigger, namespace: Namespace) => void;
  onCreateTrigger: (namespace: Namespace) => void;
  onOpenEventSource: (event: DatabaseEvent, namespace: Namespace) => void;
  onCreateEvent: (namespace: Namespace) => void;
  onOpenSequenceSource: (sequence: Sequence, namespace: Namespace) => void;
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
  onUpdateTab,
  onScheduleRecoverySave,
  onOpenRoutineSource,
  onCreateRoutine,
  onOpenTriggerSource,
  onCreateTrigger,
  onOpenEventSource,
  onCreateEvent,
  onOpenSequenceSource,
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
        onOpenRoutineSource={onOpenRoutineSource}
        onCreateRoutine={onCreateRoutine}
        onOpenTriggerSource={onOpenTriggerSource}
        onCreateTrigger={onCreateTrigger}
        onOpenEventSource={onOpenEventSource}
        onCreateEvent={onCreateEvent}
        onOpenSequenceSource={onOpenSequenceSource}
        onClose={() => onCloseTab(activeTab.id)}
      />
    );
  }

  if (activeTab?.type === 'snapshots') {
    return (
      <div className="flex-1 min-h-0 flex flex-col">
        <SnapshotManager
          key={activeTab.id}
          onCompareInDiff={(snapshotId, meta) => {
            const source = {
              type: 'snapshot' as const,
              label: meta.name,
              snapshotId,
              namespace: meta.namespace,
            };
            onOpenTab(createDiffTab(source, undefined, `Data Diff: ${meta.name}`));
          }}
        />
      </div>
    );
  }

  if (activeTab?.type === 'notebook') {
    return (
      <div className="h-full">
        <NotebookTab
          key={activeTab.id}
          tabId={activeTab.id}
          sessionId={sessionId}
          dialect={driver}
          driverCapabilities={driverCapabilities}
          environment={activeConnection?.environment || 'development'}
          readOnly={activeConnection?.read_only || false}
          connectionName={activeConnection?.name}
          connectionDatabase={activeConnection?.database}
          activeNamespace={activeTab.namespace}
          initialPath={activeTab.notebookPath}
          initialQuery={queryDrafts[activeTab.id] ?? activeTab.initialQuery}
          onSchemaChange={onSchemaChange}
          onDirtyChange={dirty => onUpdateTab(activeTab.id, { notebookDirty: dirty })}
        />
      </div>
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
      <ErrorBoundary>
        <div className="flex h-full min-h-0 min-w-0 flex-col overflow-hidden">
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
      </ErrorBoundary>
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
