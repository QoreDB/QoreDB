/**
 * useDiffSources - Hook for managing diff sources state and execution
 */
import { useState, useCallback, useMemo, useEffect, useRef, MutableRefObject } from 'react';
import { DiffSource } from '@/lib/tabs';
import {
  QueryResult,
  Namespace,
  executeQuery,
  previewTable,
  connectSavedConnection,
  disconnect,
  listNamespaces,
  SavedConnection,
} from '@/lib/tauri';
import { compareResults, DiffResult, findCommonColumns } from '@/lib/diffUtils';
import { DiffSourceState } from '../DiffSourcePanel';

const DEFAULT_PROJECT = 'default';

const TRIVIAL_COLUMN_SET = new Set([
  'id',
  'created',
  'updated',
  'createdat',
  'updatedat',
  'createdon',
  'updatedon',
  'createddate',
  'updateddate',
]);

function normalizeColumnName(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]/g, '');
}

function isTrivialColumn(name: string): boolean {
  return TRIVIAL_COLUMN_SET.has(normalizeColumnName(name));
}

function getNamespaceKey(ns: Namespace): string {
  return `${ns.database}:${ns.schema ?? ''}`;
}

function findMatchingNamespace(
  namespaces: Namespace[],
  target?: Namespace
): Namespace | null {
  if (!target) return null;
  return (
    namespaces.find(
      ns =>
        ns.database === target.database &&
        (ns.schema || '') === (target.schema || '')
    ) || null
  );
}

function resolveDefaultNamespace(
  namespaces: Namespace[],
  preferredNamespace?: Namespace,
  preferredDatabase?: string
): Namespace | null {
  if (!namespaces.length) return null;
  const preferredMatch = findMatchingNamespace(namespaces, preferredNamespace);
  if (preferredMatch) return preferredMatch;

  if (preferredDatabase) {
    const matches = namespaces.filter(ns => ns.database === preferredDatabase);
    if (matches.length > 0) {
      return matches.find(ns => ns.schema === 'public') || matches[0];
    }
  }

  return namespaces[0];
}

export interface UseDiffSourcesOptions {
  activeConnection?: SavedConnection | null;
  initialNamespace?: Namespace;
  initialLeftSource?: DiffSource;
  initialRightSource?: DiffSource;
}

export interface UseDiffSourcesReturn {
  // Source states
  leftSource: DiffSourceState;
  rightSource: DiffSourceState;

  // Connection + namespace
  setLeftConnection: (connection: SavedConnection | null) => Promise<void>;
  setRightConnection: (connection: SavedConnection | null) => Promise<void>;
  setLeftNamespace: (namespace: Namespace | null) => void;
  setRightNamespace: (namespace: Namespace | null) => void;

  // Source setters
  updateLeftSource: (updates: Partial<DiffSourceState>) => void;
  updateRightSource: (updates: Partial<DiffSourceState>) => void;

  // Execution
  executeLeft: () => Promise<void>;
  executeRight: () => Promise<void>;
  executeBoth: () => Promise<void>;

  // Comparison
  keyColumns: string[];
  setKeyColumns: (columns: string[]) => void;
  compare: () => void;
  diffResult: DiffResult | null;
  comparing: boolean;
  commonColumns: { name: string; data_type: string }[];
  trivialCommonColumns: string[];
  compareBlockedReason: 'missingResults' | null;
  compareWarning: 'noCommonColumns' | 'trivialCommonColumns' | null;

  // Actions
  swap: () => void;
  refresh: () => Promise<void>;
  reset: () => void;

  // Derived state
  canCompare: boolean;
  hasResults: boolean;
}

function initSourceState(
  source: DiffSource | undefined,
  activeConnection: SavedConnection | null | undefined,
  initialNamespace?: Namespace
): DiffSourceState {
  const connectionId = source?.connectionId ?? activeConnection?.id;
  const connection = connectionId === activeConnection?.id ? activeConnection : undefined;

  return {
    mode: source?.type === 'query' ? 'query' : 'table',
    connectionId,
    connection,
    tableName: source?.tableName,
    query: source?.query,
    result: source?.result,
    namespace: source?.namespace ?? initialNamespace,
    namespaces: undefined,
    sessionId: undefined,
    loading: false,
    connecting: false,
    namespacesLoading: false,
    error: undefined,
    connectionError: undefined,
  };
}

export function useDiffSources({
  activeConnection,
  initialNamespace,
  initialLeftSource,
  initialRightSource,
}: UseDiffSourcesOptions): UseDiffSourcesReturn {
  const [leftSource, setLeftSource] = useState<DiffSourceState>(() =>
    initSourceState(initialLeftSource, activeConnection, initialNamespace)
  );
  const [rightSource, setRightSource] = useState<DiffSourceState>(() =>
    initSourceState(initialRightSource, activeConnection, initialNamespace)
  );

  const sharedSessionsRef = useRef<Map<string, { sessionId: string; refs: number }>>(
    new Map()
  );
  const leftConnectAttemptRef = useRef(0);
  const rightConnectAttemptRef = useRef(0);
  const leftExecAttemptRef = useRef(0);
  const rightExecAttemptRef = useRef(0);
  const leftConnectionIdRef = useRef<string | undefined>(leftSource.connectionId);
  const rightConnectionIdRef = useRef<string | undefined>(rightSource.connectionId);

  const [keyColumns, setKeyColumns] = useState<string[]>([]);
  const [diffResult, setDiffResult] = useState<DiffResult | null>(null);
  const [comparing, setComparing] = useState(false);

  const releaseConnection = useCallback(async (connectionId?: string) => {
    if (!connectionId) return;
    const entry = sharedSessionsRef.current.get(connectionId);
    if (!entry) return;
    entry.refs -= 1;
    if (entry.refs > 0) return;
    sharedSessionsRef.current.delete(connectionId);
    try {
      await disconnect(entry.sessionId);
    } catch (err) {
      console.warn('Failed to disconnect diff session', err);
    }
  }, []);

  const acquireSession = useCallback(async (connection: SavedConnection): Promise<string> => {
    const existing = sharedSessionsRef.current.get(connection.id);
    if (existing) {
      existing.refs += 1;
      return existing.sessionId;
    }

    const result = await connectSavedConnection(DEFAULT_PROJECT, connection.id);
    if (!result.success || !result.session_id) {
      throw new Error(result.error || 'Failed to connect');
    }

    sharedSessionsRef.current.set(connection.id, { sessionId: result.session_id, refs: 1 });
    return result.session_id;
  }, []);

  const updateLeftSource = useCallback((updates: Partial<DiffSourceState>) => {
    setLeftSource(prev => {
      const next = { ...prev, ...updates };
      if (
        'tableName' in updates ||
        'query' in updates ||
        'mode' in updates ||
        'connectionId' in updates ||
        'namespace' in updates
      ) {
        next.result = undefined;
        next.error = undefined;
        next.loading = false;
      }
      return next;
    });
    if (
      'tableName' in updates ||
      'query' in updates ||
      'mode' in updates ||
      'connectionId' in updates ||
      'namespace' in updates
    ) {
      setDiffResult(null);
    }
  }, []);

  const updateRightSource = useCallback((updates: Partial<DiffSourceState>) => {
    setRightSource(prev => {
      const next = { ...prev, ...updates };
      if (
        'tableName' in updates ||
        'query' in updates ||
        'mode' in updates ||
        'connectionId' in updates ||
        'namespace' in updates
      ) {
        next.result = undefined;
        next.error = undefined;
        next.loading = false;
      }
      return next;
    });
    if (
      'tableName' in updates ||
      'query' in updates ||
      'mode' in updates ||
      'connectionId' in updates ||
      'namespace' in updates
    ) {
      setDiffResult(null);
    }
  }, []);

  const loadNamespacesForSource = useCallback(
    async (
      sessionId: string,
      connection: SavedConnection | undefined,
      preferredNamespace: Namespace | undefined
    ): Promise<{ namespaces: Namespace[]; namespace: Namespace | null }> => {
      const response = await listNamespaces(sessionId);
      if (!response.success || !response.namespaces) {
        throw new Error(response.error || 'Failed to load namespaces');
      }
      const namespaces = response.namespaces;
      const namespace = resolveDefaultNamespace(
        response.namespaces,
        preferredNamespace,
        connection?.database
      );
      return { namespaces, namespace };
    },
    []
  );

  const connectSource = useCallback(
    async (
      side: 'left' | 'right',
      connection: SavedConnection | null
    ) => {
      const isLeft = side === 'left';
      const attemptRef = isLeft ? leftConnectAttemptRef : rightConnectAttemptRef;
      const updateFn = isLeft ? updateLeftSource : updateRightSource;
      const currentSource = isLeft ? leftSource : rightSource;

      if (!connection) {
        if (currentSource.connectionId) {
          await releaseConnection(currentSource.connectionId);
        }
        updateFn({
          connectionId: undefined,
          connection: undefined,
          sessionId: undefined,
          namespaces: undefined,
          namespace: undefined,
          connecting: false,
          namespacesLoading: false,
          connectionError: undefined,
          tableName: undefined,
          query: undefined,
          result: undefined,
          error: undefined,
        });
        return;
      }

      if (currentSource.connectionId === connection.id && currentSource.sessionId) {
        return;
      }

      const prevConnectionId = currentSource.connectionId;
      const isSameConnection = currentSource.connectionId === connection.id;
      attemptRef.current += 1;
      const attemptId = attemptRef.current;

      const updates: Partial<DiffSourceState> = {
        connectionId: connection.id,
        connection,
        connecting: true,
        namespacesLoading: true,
        connectionError: undefined,
        sessionId: undefined,
      };

      if (!isSameConnection) {
        updates.namespaces = undefined;
        updates.namespace = undefined;
        updates.tableName = undefined;
        updates.query = undefined;
        updates.result = undefined;
        updates.error = undefined;
      }

      updateFn(updates);

      if (prevConnectionId && prevConnectionId !== connection.id) {
        await releaseConnection(prevConnectionId);
      }

      try {
        const sessionId = await acquireSession(connection);
        if (attemptRef.current !== attemptId) {
          await releaseConnection(connection.id);
          return;
        }

        const { namespaces, namespace } = await loadNamespacesForSource(
          sessionId,
          connection,
          isSameConnection ? currentSource.namespace : undefined
        );

        if (attemptRef.current !== attemptId) {
          await releaseConnection(connection.id);
          return;
        }

        updateFn({
          sessionId,
          namespaces,
          namespace: namespace ?? undefined,
          connecting: false,
          namespacesLoading: false,
        });
      } catch (err) {
        if (attemptRef.current !== attemptId) return;
        updateFn({
          connecting: false,
          namespacesLoading: false,
          connectionError: err instanceof Error ? err.message : String(err),
        });
      }
    },
    [
      acquireSession,
      leftSource,
      rightSource,
      loadNamespacesForSource,
      releaseConnection,
      updateLeftSource,
      updateRightSource,
    ]
  );

  const setLeftConnection = useCallback(
    async (connection: SavedConnection | null) => {
      await connectSource('left', connection);
    },
    [connectSource]
  );

  const setRightConnection = useCallback(
    async (connection: SavedConnection | null) => {
      await connectSource('right', connection);
    },
    [connectSource]
  );

  const setLeftNamespace = useCallback(
    (namespace: Namespace | null) => {
      updateLeftSource({
        namespace: namespace ?? undefined,
        tableName: undefined,
        result: undefined,
        error: undefined,
      });
    },
    [updateLeftSource]
  );

  const setRightNamespace = useCallback(
    (namespace: Namespace | null) => {
      updateRightSource({
        namespace: namespace ?? undefined,
        tableName: undefined,
        result: undefined,
        error: undefined,
      });
    },
    [updateRightSource]
  );

  useEffect(() => {
    if (leftSource.connection && !leftSource.sessionId && !leftSource.connecting) {
      connectSource('left', leftSource.connection).catch(() => undefined);
    }
  }, [leftSource.connection, leftSource.sessionId, leftSource.connecting, connectSource]);

  useEffect(() => {
    if (rightSource.connection && !rightSource.sessionId && !rightSource.connecting) {
      connectSource('right', rightSource.connection).catch(() => undefined);
    }
  }, [rightSource.connection, rightSource.sessionId, rightSource.connecting, connectSource]);

  useEffect(() => {
    leftConnectionIdRef.current = leftSource.connectionId;
  }, [leftSource.connectionId]);

  useEffect(() => {
    rightConnectionIdRef.current = rightSource.connectionId;
  }, [rightSource.connectionId]);

  useEffect(() => {
    return () => {
      const connections = new Set<string>();
      if (leftConnectionIdRef.current) connections.add(leftConnectionIdRef.current);
      if (rightConnectionIdRef.current) connections.add(rightConnectionIdRef.current);
      connections.forEach(connectionId => {
        const entry = sharedSessionsRef.current.get(connectionId);
        if (!entry) return;
        entry.refs -= 1;
        if (entry.refs > 0) return;
        sharedSessionsRef.current.delete(connectionId);
        disconnect(entry.sessionId).catch(err => {
          console.warn('Failed to disconnect diff session', err);
        });
      });
    };
  }, []);

  const executeSource = useCallback(
    async (
      source: DiffSourceState,
      updateFn: (updates: Partial<DiffSourceState>) => void,
      attemptRef: MutableRefObject<number>
    ) => {
      if (!source.sessionId || !source.namespace) return;

      attemptRef.current += 1;
      const attemptId = attemptRef.current;

      updateFn({ loading: true, error: undefined });

      try {
        let result: QueryResult | undefined;

        if (source.mode === 'table' && source.tableName) {
          const response = await previewTable(
            source.sessionId,
            source.namespace,
            source.tableName,
            10000
          );
          if (response.success && response.result) {
            result = response.result;
          } else {
            updateFn({ error: response.error, loading: false });
            return;
          }
        } else if (source.mode === 'query' && source.query?.trim()) {
          const response = await executeQuery(source.sessionId, source.query, {
            namespace: source.namespace,
          });
          if (response.success && response.result) {
            result = response.result;
          } else {
            updateFn({ error: response.error, loading: false });
            return;
          }
        }

        if (attemptRef.current !== attemptId) return;
        updateFn({ result, loading: false });
      } catch (err) {
        if (attemptRef.current !== attemptId) return;
        updateFn({
          error: err instanceof Error ? err.message : String(err),
          loading: false,
        });
      }
    },
    []
  );

  const executeLeft = useCallback(async () => {
    await executeSource(leftSource, updateLeftSource, leftExecAttemptRef);
  }, [leftSource, executeSource, updateLeftSource]);

  const executeRight = useCallback(async () => {
    await executeSource(rightSource, updateRightSource, rightExecAttemptRef);
  }, [rightSource, executeSource, updateRightSource]);

  const executeBoth = useCallback(async () => {
    await Promise.all([executeLeft(), executeRight()]);
  }, [executeLeft, executeRight]);

  useEffect(() => {
    if (
      leftSource.mode !== 'table' ||
      !leftSource.tableName ||
      !leftSource.sessionId ||
      !leftSource.namespace ||
      leftSource.loading ||
      leftSource.result ||
      leftSource.error
    ) {
      return;
    }

    executeLeft().catch(() => undefined);
  }, [
    leftSource.mode,
    leftSource.tableName,
    leftSource.sessionId,
    leftSource.namespace,
    leftSource.loading,
    leftSource.result,
    leftSource.error,
    executeLeft,
  ]);

  useEffect(() => {
    if (
      rightSource.mode !== 'table' ||
      !rightSource.tableName ||
      !rightSource.sessionId ||
      !rightSource.namespace ||
      rightSource.loading ||
      rightSource.result ||
      rightSource.error
    ) {
      return;
    }

    executeRight().catch(() => undefined);
  }, [
    rightSource.mode,
    rightSource.tableName,
    rightSource.sessionId,
    rightSource.namespace,
    rightSource.loading,
    rightSource.result,
    rightSource.error,
    executeRight,
  ]);

  const commonColumns = useMemo(() => {
    if (!leftSource.result || !rightSource.result) return [];
    return findCommonColumns(leftSource.result, rightSource.result);
  }, [leftSource.result, rightSource.result]);

  useEffect(() => {
    if (!leftSource.result || !rightSource.result) return;
    const commonNames = new Set(commonColumns.map(col => col.name));
    setKeyColumns(prev => {
      const next = prev.filter(name => commonNames.has(name));
      return next.length === prev.length ? prev : next;
    });
  }, [leftSource.result, rightSource.result, commonColumns]);

  const compareBlockedReason = useMemo(() => {
    if (!leftSource.result || !rightSource.result) return 'missingResults';
    return null;
  }, [leftSource.result, rightSource.result]);

  const trivialCommonColumns = useMemo(
    () => commonColumns.filter(col => isTrivialColumn(col.name)).map(col => col.name),
    [commonColumns]
  );

  const compareWarning = useMemo(() => {
    if (commonColumns.length === 0) return 'noCommonColumns';
    if (trivialCommonColumns.length === commonColumns.length) {
      return 'trivialCommonColumns';
    }
    return null;
  }, [commonColumns.length, trivialCommonColumns.length]);

  const compare = useCallback(() => {
    if (compareBlockedReason || !leftSource.result || !rightSource.result) return;

    setComparing(true);
    try {
      const result = compareResults(
        leftSource.result,
        rightSource.result,
        keyColumns.length > 0 ? keyColumns : undefined
      );
      setDiffResult(result);
    } finally {
      setComparing(false);
    }
  }, [leftSource.result, rightSource.result, keyColumns, compareBlockedReason]);

  const swap = useCallback(() => {
    setLeftSource(rightSource);
    setRightSource(leftSource);
    setDiffResult(null);
  }, [leftSource, rightSource]);

  const refresh = useCallback(async () => {
    await executeBoth();
    if (diffResult) {
      setTimeout(compare, 100);
    }
  }, [executeBoth, diffResult, compare]);

  const reset = useCallback(() => {
    releaseConnection(leftSource.connectionId).catch(() => undefined);
    releaseConnection(rightSource.connectionId).catch(() => undefined);
    setLeftSource({
      mode: 'table',
      loading: false,
      connecting: false,
      namespacesLoading: false,
    });
    setRightSource({
      mode: 'table',
      loading: false,
      connecting: false,
      namespacesLoading: false,
    });
    setKeyColumns([]);
    setDiffResult(null);
  }, [leftSource.connectionId, rightSource.connectionId, releaseConnection]);

  const canCompare = useMemo(
    () => compareBlockedReason === null,
    [compareBlockedReason]
  );

  const hasResults = useMemo(
    () => Boolean(leftSource.result || rightSource.result),
    [leftSource.result, rightSource.result]
  );

  return {
    leftSource,
    rightSource,
    setLeftConnection,
    setRightConnection,
    setLeftNamespace,
    setRightNamespace,
    updateLeftSource,
    updateRightSource,
    executeLeft,
    executeRight,
    executeBoth,
    keyColumns,
    setKeyColumns,
    compare,
    diffResult,
    comparing,
    commonColumns,
    trivialCommonColumns,
    compareBlockedReason,
    compareWarning,
    swap,
    refresh,
    reset,
    canCompare,
    hasResults,
  };
}
