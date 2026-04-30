// SPDX-License-Identifier: Apache-2.0

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { check } from '@tauri-apps/plugin-updater';
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useTranslation } from 'react-i18next';
import type { DatabaseBrowserTab } from '@/components/Browser/DatabaseBrowser';
import type { TableBrowserTab } from '@/components/Browser/TableBrowser';
import { useRecovery } from '@/hooks/useRecovery';
import { Driver } from '@/lib/connection/drivers';
import {
  type CrashRecoverySnapshot,
  saveCrashRecoverySnapshot,
} from '@/lib/diagnostics/crashRecovery';
import { shouldSaveQueryDrafts } from '@/lib/diagnostics/crashRecoverySettings';
import { UI_EVENT_CONNECTIONS_CHANGED, UI_EVENT_WORKSPACE_CHANGED } from '@/lib/events/uiEvents';
import { notify } from '@/lib/notify';
import {
  handleCloseConnectionModal as closeConnectionModal,
  setSettingsOpen,
} from '@/lib/stores/modalStore';
import { setUpdateAvailable } from '@/lib/stores/updateStore';
import type { OpenTab } from '@/lib/tabs';
import {
  type ConnectionHealth,
  type ConnectionHealthEvent,
  connectSavedConnection,
  type DriverCapabilities,
  disconnect,
  getDriverInfo,
  listSavedConnections,
  type SavedConnection,
} from '@/lib/tauri';
import { useTabContext } from './TabProvider';
import { useWorkspace } from './WorkspaceProvider';

const RECOVERY_SAVE_DEBOUNCE_MS = 600;
const STARTUP_PREFS_KEY = 'qoredb_startup_preferences';

function shouldCheckUpdatesOnStartup(): boolean {
  try {
    const stored = localStorage.getItem(STARTUP_PREFS_KEY);
    if (!stored) return true;
    const parsed = JSON.parse(stored) as { checkUpdates?: unknown };
    return typeof parsed.checkUpdates === 'boolean' ? parsed.checkUpdates : true;
  } catch {
    return true;
  }
}

function getConnectionSignature(connection: SavedConnection): string {
  return JSON.stringify({
    driver: connection.driver,
    host: connection.host,
    port: connection.port,
    username: connection.username,
    database: connection.database ?? null,
    ssl: connection.ssl,
    pool_max_connections: connection.pool_max_connections ?? null,
    pool_min_connections: connection.pool_min_connections ?? null,
    pool_acquire_timeout_secs: connection.pool_acquire_timeout_secs ?? null,
    ssh_tunnel: connection.ssh_tunnel
      ? {
          host: connection.ssh_tunnel.host,
          port: connection.ssh_tunnel.port,
          username: connection.ssh_tunnel.username,
          auth_type: connection.ssh_tunnel.auth_type,
          key_path: connection.ssh_tunnel.key_path ?? null,
          host_key_policy: connection.ssh_tunnel.host_key_policy,
          proxy_jump: connection.ssh_tunnel.proxy_jump ?? null,
          connect_timeout_secs: connection.ssh_tunnel.connect_timeout_secs,
          keepalive_interval_secs: connection.ssh_tunnel.keepalive_interval_secs,
          keepalive_count_max: connection.ssh_tunnel.keepalive_count_max,
        }
      : null,
  });
}

export interface SessionContextValue {
  sessionId: string | null;
  driver: Driver;
  driverCapabilities: DriverCapabilities | null;
  activeConnection: SavedConnection | null;
  connectionHealth: ConnectionHealth;
  hasConnections: boolean;
  sidebarRefreshTrigger: number;
  schemaRefreshTrigger: number;
  recovery: ReturnType<typeof useRecovery>;
  handleConnected: (
    sessionId: string,
    connection: SavedConnection,
    options?: {
      tabs?: OpenTab[];
      activeTabId?: string | null;
      queryDrafts?: Record<string, string>;
      tableBrowserTabs?: Record<string, TableBrowserTab>;
      databaseBrowserTabs?: Record<string, DatabaseBrowserTab>;
    }
  ) => void;
  handleRestoreSession: () => Promise<void>;
  handleConnectionSaved: (connection: SavedConnection) => void;
  refreshSidebar: () => void;
  triggerSchemaRefresh: () => void;
  scheduleRecoverySave: () => void;
}

const SessionContext = createContext<SessionContextValue | null>(null);

export function SessionProvider({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const { tabs, activeTabId, queryDrafts, tableBrowserTabs, databaseBrowserTabs, resetTabs } =
    useTabContext();
  const { projectId } = useWorkspace();
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [driver, setDriver] = useState<Driver>(Driver.Postgres);
  const [driverCapabilities, setDriverCapabilities] = useState<DriverCapabilities | null>(null);
  const [activeConnection, setActiveConnection] = useState<SavedConnection | null>(null);
  const [hasConnections, setHasConnections] = useState(false);
  const [connectionHealth, setConnectionHealth] = useState<ConnectionHealth>('healthy');
  const [sidebarRefreshTrigger, setSidebarRefreshTrigger] = useState(0);
  const [schemaRefreshTrigger, setSchemaRefreshTrigger] = useState(0);

  const recoverySaveHandleRef = useRef<number | null>(null);
  const reconnectAttemptRef = useRef(0);
  const pendingReconnectRef = useRef<string | null>(null);

  const recovery = useRecovery();

  // Load saved connections on mount & refresh
  useEffect(() => {
    listSavedConnections(projectId)
      .then(saved => setHasConnections(saved.length > 0))
      .catch(() => setHasConnections(false));
  }, [projectId]);

  // Listen for connections-changed events
  useEffect(() => {
    const handler = () => setSidebarRefreshTrigger(prev => prev + 1);
    window.addEventListener(UI_EVENT_CONNECTIONS_CHANGED, handler);
    return () => window.removeEventListener(UI_EVENT_CONNECTIONS_CHANGED, handler);
  }, []);

  // Disconnect and reset when workspace changes
  useEffect(() => {
    const handler = () => {
      const currentSessionId = sessionId;
      if (currentSessionId) {
        disconnect(currentSessionId).catch(err =>
          console.warn('Failed to disconnect on workspace switch:', err)
        );
      }
      setSessionId(null);
      setActiveConnection(null);
      setDriverCapabilities(null);
      setConnectionHealth('healthy');
      resetTabs();
      setSidebarRefreshTrigger(prev => prev + 1);
    };
    window.addEventListener(UI_EVENT_WORKSPACE_CHANGED, handler);
    return () => window.removeEventListener(UI_EVENT_WORKSPACE_CHANGED, handler);
  }, [sessionId, resetTabs]);

  // Check for updates on startup
  useEffect(() => {
    if (!import.meta.env.PROD) return;
    if (!shouldCheckUpdatesOnStartup()) return;

    let cancelled = false;
    const handle = window.setTimeout(async () => {
      try {
        const update = await check();
        if (!update || cancelled) return;
        setUpdateAvailable(update);
      } catch (err) {
        console.warn('Update check failed', err);
      }
    }, 4000);

    return () => {
      cancelled = true;
      window.clearTimeout(handle);
    };
  }, []);

  // Listen for connection health events from Tauri backend
  useEffect(() => {
    if (!sessionId) {
      setConnectionHealth('healthy');
      return;
    }

    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    listen<ConnectionHealthEvent>('connection_health', event => {
      if (cancelled) return;
      if (event.payload.session_id === sessionId) {
        const newHealth = event.payload.health;
        setConnectionHealth(prev => {
          if (newHealth === 'unhealthy' && prev !== 'unhealthy') {
            notify.warning(t('status.connectionUnhealthy'));
          } else if (newHealth === 'healthy' && prev !== 'healthy') {
            notify.success(t('status.connectionRestored'));
          }
          return newHealth;
        });
      }
    }).then(fn => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [sessionId, t]);

  // Fetch driver capabilities when session changes
  useEffect(() => {
    if (!sessionId) {
      setDriverCapabilities(null);
      return;
    }
    let cancelled = false;
    getDriverInfo(sessionId)
      .then(response => {
        if (cancelled) return;
        setDriverCapabilities(
          response.success && response.driver ? response.driver.capabilities : null
        );
      })
      .catch(() => {
        if (!cancelled) setDriverCapabilities(null);
      });
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  // Recovery save scheduling
  const scheduleRecoverySave = useCallback(() => {
    if (!activeConnection || !sessionId) return;

    const snapshot: CrashRecoverySnapshot = {
      updatedAt: Date.now(),
      projectId: projectId,
      connectionId: activeConnection.id,
      activeTabId,
      tabs: tabs.map(tab => ({
        id: tab.id,
        type: tab.type,
        title: tab.title,
        namespace: tab.namespace,
        tableName: tab.tableName,
      })),
      queryDrafts: shouldSaveQueryDrafts() ? queryDrafts : {},
      tableBrowserTabs: { ...tableBrowserTabs },
      databaseBrowserTabs: { ...databaseBrowserTabs },
    };

    if (recoverySaveHandleRef.current) {
      window.clearTimeout(recoverySaveHandleRef.current);
    }
    recoverySaveHandleRef.current = window.setTimeout(() => {
      saveCrashRecoverySnapshot(snapshot);
    }, RECOVERY_SAVE_DEBOUNCE_MS);
  }, [
    activeConnection,
    sessionId,
    projectId,
    tabs,
    activeTabId,
    queryDrafts,
    tableBrowserTabs,
    databaseBrowserTabs,
  ]);

  useEffect(() => {
    scheduleRecoverySave();
    return () => {
      if (recoverySaveHandleRef.current) {
        window.clearTimeout(recoverySaveHandleRef.current);
      }
    };
  }, [scheduleRecoverySave]);

  const refreshSidebar = useCallback(() => {
    setSidebarRefreshTrigger(prev => prev + 1);
  }, []);

  const triggerSchemaRefresh = useCallback(() => {
    setSchemaRefreshTrigger(prev => prev + 1);
  }, []);

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
      reconnectAttemptRef.current += 1;
      pendingReconnectRef.current = null;
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

  const handleConnectionSaved = useCallback(
    (updatedConnection: SavedConnection) => {
      const isActive = activeConnection?.id === updatedConnection.id;
      const shouldReconnect =
        (Boolean(isActive && sessionId && activeConnection) &&
          getConnectionSignature(activeConnection as SavedConnection) !==
            getConnectionSignature(updatedConnection)) ||
        pendingReconnectRef.current === updatedConnection.id;

      if (isActive) {
        setActiveConnection(prev => (prev ? { ...prev, ...updatedConnection } : updatedConnection));
        setDriver(updatedConnection.driver as Driver);
      }

      closeConnectionModal();
      setSidebarRefreshTrigger(prev => prev + 1);

      if (shouldReconnect) {
        const previousSessionId = sessionId;
        reconnectAttemptRef.current += 1;
        const attemptId = reconnectAttemptRef.current;
        void (async () => {
          try {
            const reconnectResult = await connectSavedConnection(projectId, updatedConnection.id);
            if (attemptId !== reconnectAttemptRef.current) return;
            if (reconnectResult.success && reconnectResult.session_id) {
              pendingReconnectRef.current = null;
              handleConnected(reconnectResult.session_id, updatedConnection);
              try {
                if (previousSessionId) await disconnect(previousSessionId);
              } catch (err) {
                console.warn('Failed to disconnect previous session', err);
              }
            } else {
              notify.error(
                t('sidebar.connectionToFailed', { name: updatedConnection.name }),
                reconnectResult.error
              );
              pendingReconnectRef.current = updatedConnection.id;
              try {
                if (previousSessionId) await disconnect(previousSessionId);
              } catch (err) {
                console.warn('Failed to disconnect previous session after reconnect failure', err);
              }
              setSessionId(null);
              setActiveConnection(null);
              resetTabs();
            }
          } catch (err) {
            if (attemptId !== reconnectAttemptRef.current) return;
            notify.error(t('sidebar.connectError'), err);
            pendingReconnectRef.current = updatedConnection.id;
            try {
              if (previousSessionId) await disconnect(previousSessionId);
            } catch (disconnectErr) {
              console.warn(
                'Failed to disconnect previous session after reconnect error',
                disconnectErr
              );
            }
            setSessionId(null);
            setActiveConnection(null);
            resetTabs();
          }
        })();
      }
    },
    [activeConnection, handleConnected, projectId, resetTabs, sessionId, t]
  );

  const value = useMemo<SessionContextValue>(
    () => ({
      sessionId,
      driver,
      driverCapabilities,
      activeConnection,
      connectionHealth,
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
    }),
    [
      sessionId,
      driver,
      driverCapabilities,
      activeConnection,
      connectionHealth,
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
    ]
  );

  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

export function useSessionContext(): SessionContextValue {
  const ctx = useContext(SessionContext);
  if (!ctx) throw new Error('useSessionContext must be used within SessionProvider');
  return ctx;
}
