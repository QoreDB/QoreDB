// SPDX-License-Identifier: BUSL-1.1

import { type MutableRefObject, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import i18n from '@/i18n';
import { compareResults, type DiffResult, findCommonColumns } from '@/lib/diffUtils';
import type { DiffSource } from '@/lib/tabs';
import {
  type ColumnInfo,
  connectSavedConnection,
  describeTable,
  disconnect,
  executeQuery,
  getSnapshot,
  listNamespaces,
  type Namespace,
  previewTable,
  type SavedConnection,
  type TableSchema,
} from '@/lib/tauri';
import { useWorkspace } from '@/providers/WorkspaceProvider';
import type { DiffSourceState } from '../DiffSourcePanel';

type DiffColumnMetadata = {
  dataType: string;
  isPrimaryKey: boolean;
  isForeignKey: boolean;
  isUnique: boolean;
};

const EMPTY_COLUMN_METADATA = new Map<string, DiffColumnMetadata>();
const AUDIT_COLUMN_ACTIONS = new Set(['created', 'updated', 'modified', 'deleted', 'inserted']);
const AUDIT_COLUMN_TIME_HINTS = new Set(['at', 'on', 'date', 'time', 'timestamp']);

function tokenizeColumnName(name: string): string[] {
  return name
    .replace(/([a-z0-9])([A-Z])/g, '$1_$2')
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean);
}

function isTemporalColumnType(dataType: string): boolean {
  const normalized = dataType.toLowerCase();
  return (
    normalized.includes('timestamp') ||
    normalized.includes('datetime') ||
    normalized.includes('date') ||
    normalized.includes('time')
  );
}

function isLikelyAuditColumn(name: string, dataType: string): boolean {
  if (!isTemporalColumnType(dataType)) return false;

  const tokens = tokenizeColumnName(name);
  if (tokens.length === 0) return false;

  const actionIndex = tokens.findIndex(token => AUDIT_COLUMN_ACTIONS.has(token));
  if (actionIndex === -1 || actionIndex > 1) return false;

  if (tokens.length === 1) return true;
  return AUDIT_COLUMN_TIME_HINTS.has(tokens[tokens.length - 1]);
}

function buildColumnMetadataMap(schema: TableSchema | null): Map<string, DiffColumnMetadata> {
  if (!schema) return EMPTY_COLUMN_METADATA;

  const primaryKeys = new Set(schema.primary_key ?? []);
  const foreignKeys = new Set(schema.foreign_keys.map(fk => fk.column));
  const uniqueColumns = new Set<string>();

  schema.indexes.forEach(index => {
    if (!index.is_unique) return;
    index.columns.forEach(column => uniqueColumns.add(column));
  });

  return new Map(
    schema.columns.map(column => [
      column.name,
      {
        dataType: column.data_type,
        isPrimaryKey: column.is_primary_key || primaryKeys.has(column.name),
        isForeignKey: foreignKeys.has(column.name),
        isUnique: uniqueColumns.has(column.name),
      },
    ])
  );
}

function isTrivialColumn(column: ColumnInfo, metadata?: DiffColumnMetadata): boolean {
  if (metadata && (metadata.isPrimaryKey || metadata.isForeignKey || metadata.isUnique)) {
    return false;
  }

  return isLikelyAuditColumn(column.name, metadata?.dataType ?? column.data_type);
}

function getTableSchemaCacheKey(
  sessionId: string,
  namespace: Namespace,
  tableName: string
): string {
  return `${sessionId}:${namespace.database}:${namespace.schema ?? ''}:${tableName}`;
}

function findMatchingNamespace(namespaces: Namespace[], target?: Namespace): Namespace | null {
  if (!target) return null;
  return (
    namespaces.find(
      ns => ns.database === target.database && (ns.schema || '') === (target.schema || '')
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

/**
 * Table-mode diffs read a bounded page, so a large table is compared on a
 * prefix rather than in full. The bound is surfaced and adjustable because a
 * silent one turns a partial answer into an apparently complete one.
 */
export const DIFF_ROW_LIMITS = [1_000, 5_000, 10_000, 25_000] as const;
export const DEFAULT_DIFF_ROW_LIMIT = DIFF_ROW_LIMITS[0];

export interface UseDiffSourcesOptions {
  activeConnection?: SavedConnection | null;
  initialNamespace?: Namespace;
  initialLeftSource?: DiffSource;
  initialRightSource?: DiffSource;
}

export interface UseDiffSourcesReturn {
  leftSource: DiffSourceState;
  rightSource: DiffSourceState;

  setLeftConnection: (connection: SavedConnection | null) => Promise<void>;
  setRightConnection: (connection: SavedConnection | null) => Promise<void>;
  setLeftNamespace: (namespace: Namespace | null) => void;
  setRightNamespace: (namespace: Namespace | null) => void;

  updateLeftSource: (updates: Partial<DiffSourceState>) => void;
  updateRightSource: (updates: Partial<DiffSourceState>) => void;

  executeLeft: () => Promise<void>;
  executeRight: () => Promise<void>;
  executeBoth: () => Promise<void>;

  keyColumns: string[];
  setKeyColumns: (columns: string[]) => void;
  compare: () => void;
  diffResult: DiffResult | null;
  comparing: boolean;
  commonColumns: { name: string; data_type: string }[];
  trivialCommonColumns: string[];
  compareBlockedReason: 'missingResults' | null;
  compareWarning: 'noCommonColumns' | 'trivialCommonColumns' | null;

  rowLimit: number;
  setRowLimit: (limit: number) => void;
  /** A side whose table page came back full: the diff may not be exhaustive. */
  truncatedSides: ('left' | 'right')[];

  swap: () => void;
  refresh: () => Promise<void>;
  reset: () => void;

  canCompare: boolean;
  hasResults: boolean;
}

function initSourceState(
  source: DiffSource | undefined,
  activeConnection: SavedConnection | null | undefined,
  initialNamespace?: Namespace
): DiffSourceState {
  if (source?.type === 'snapshot') {
    return {
      mode: 'snapshot',
      snapshotId: source.snapshotId,
      snapshotName: source.label,
      result: source.result,
      truncated: source.truncated,
      namespace: source.namespace ?? initialNamespace,
      loading: false,
      connecting: false,
      namespacesLoading: false,
    };
  }

  const connectionId = source?.connectionId ?? activeConnection?.id;
  const connection =
    activeConnection && connectionId === activeConnection.id ? activeConnection : undefined;

  return {
    mode: source?.type === 'query' ? 'query' : 'table',
    connectionId,
    connection,
    tableName: source?.tableName,
    query: source?.query,
    result: source?.result,
    // An imported result without limit metadata cannot establish completeness.
    truncated: source?.truncated ?? Boolean(source?.result),
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
  const { projectId } = useWorkspace();
  const [leftSource, setLeftSource] = useState<DiffSourceState>(() =>
    initSourceState(initialLeftSource, activeConnection, initialNamespace)
  );
  const [rightSource, setRightSource] = useState<DiffSourceState>(() =>
    initSourceState(initialRightSource, activeConnection, initialNamespace)
  );

  const sharedSessionsRef = useRef<
    Map<string, { promise: Promise<string>; refs: number; projectId: string }>
  >(new Map());
  const leftConnectAttemptRef = useRef(0);
  const rightConnectAttemptRef = useRef(0);
  const leftExecAttemptRef = useRef(0);
  const rightExecAttemptRef = useRef(0);
  const [rowLimit, setRowLimitState] = useState<number>(DEFAULT_DIFF_ROW_LIMIT);
  const rowLimitRef = useRef(rowLimit);
  const setRowLimit = useCallback((limit: number) => {
    rowLimitRef.current = limit;
    setRowLimitState(limit);
  }, []);
  const [keyColumns, setKeyColumnsState] = useState<string[]>([]);
  const keyColumnsRef = useRef(keyColumns);
  keyColumnsRef.current = keyColumns;
  const [diffResult, setDiffResult] = useState<DiffResult | null>(null);
  const setKeyColumns = useCallback((columns: string[]) => {
    keyColumnsRef.current = columns;
    setKeyColumnsState(columns);
    setDiffResult(null);
  }, []);
  const [comparing, setComparing] = useState(false);
  const [leftTableSchema, setLeftTableSchema] = useState<TableSchema | null>(null);
  const [rightTableSchema, setRightTableSchema] = useState<TableSchema | null>(null);
  const tableSchemaCacheRef = useRef<Map<string, TableSchema | null>>(new Map());

  const releaseConnection = useCallback(
    async (connectionId?: string) => {
      if (!connectionId) return;
      const key = JSON.stringify([projectId, connectionId]);
      const entry = sharedSessionsRef.current.get(key);
      if (!entry) return;
      entry.refs -= 1;
      if (entry.refs > 0) return;
      sharedSessionsRef.current.delete(key);
      try {
        await disconnect(await entry.promise);
      } catch (err) {
        console.warn('Failed to disconnect diff session', err);
      }
    },
    [projectId]
  );

  const acquireSession = useCallback(
    async (connection: SavedConnection): Promise<string> => {
      const key = JSON.stringify([projectId, connection.id]);
      const existing = sharedSessionsRef.current.get(key);
      if (existing) {
        existing.refs += 1;
        return existing.promise;
      }

      // Register the pending connection before awaiting it so both sides share
      // the same session even when they open concurrently.
      const promise = connectSavedConnection(projectId, connection.id).then(result => {
        if (!result.success || !result.session_id) {
          throw new Error(result.error || 'Failed to connect');
        }
        return result.session_id;
      });
      const entry = { promise, refs: 1, projectId };
      sharedSessionsRef.current.set(key, entry);
      try {
        return await promise;
      } catch (error) {
        if (sharedSessionsRef.current.get(key) === entry) sharedSessionsRef.current.delete(key);
        throw error;
      }
    },
    [projectId]
  );

  const updateLeftSource = useCallback((updates: Partial<DiffSourceState>) => {
    setLeftSource(prev => {
      const next = { ...prev, ...updates };
      if (
        'tableName' in updates ||
        'query' in updates ||
        'snapshotId' in updates ||
        'mode' in updates ||
        'connectionId' in updates ||
        'namespace' in updates
      ) {
        next.result = undefined;
        next.truncated = undefined;
        next.error = undefined;
        next.loading = false;
      }
      return next;
    });
    if (
      'tableName' in updates ||
      'query' in updates ||
      'snapshotId' in updates ||
      'mode' in updates ||
      'connectionId' in updates ||
      'namespace' in updates
    ) {
      leftExecAttemptRef.current += 1;
      setDiffResult(null);
    } else if ('result' in updates || updates.loading) {
      setDiffResult(null);
    }
  }, []);

  const updateRightSource = useCallback((updates: Partial<DiffSourceState>) => {
    setRightSource(prev => {
      const next = { ...prev, ...updates };
      if (
        'tableName' in updates ||
        'query' in updates ||
        'snapshotId' in updates ||
        'mode' in updates ||
        'connectionId' in updates ||
        'namespace' in updates
      ) {
        next.result = undefined;
        next.truncated = undefined;
        next.error = undefined;
        next.loading = false;
      }
      return next;
    });
    if (
      'tableName' in updates ||
      'query' in updates ||
      'snapshotId' in updates ||
      'mode' in updates ||
      'connectionId' in updates ||
      'namespace' in updates
    ) {
      rightExecAttemptRef.current += 1;
      setDiffResult(null);
    } else if ('result' in updates || updates.loading) {
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
    async (side: 'left' | 'right', connection: SavedConnection | null) => {
      const isLeft = side === 'left';
      const attemptRef = isLeft ? leftConnectAttemptRef : rightConnectAttemptRef;
      const updateFn = isLeft ? updateLeftSource : updateRightSource;
      const currentSource = isLeft ? leftSource : rightSource;
      attemptRef.current += 1;
      const attemptId = attemptRef.current;

      if (!connection) {
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
        await releaseConnection(currentSource.connectionId);
        return;
      }

      if (currentSource.connectionId === connection.id && currentSource.sessionId) {
        return;
      }

      const prevConnectionId = currentSource.connectionId;
      const isSameConnection = currentSource.connectionId === connection.id;

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

      let acquired = false;
      try {
        const sessionId = await acquireSession(connection);
        acquired = true;
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
        if (acquired) await releaseConnection(connection.id);
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
    if (
      leftSource.connection &&
      !leftSource.sessionId &&
      !leftSource.connecting &&
      !leftSource.connectionError
    ) {
      connectSource('left', leftSource.connection).catch(() => undefined);
    }
  }, [
    leftSource.connection,
    leftSource.sessionId,
    leftSource.connecting,
    leftSource.connectionError,
    connectSource,
  ]);

  useEffect(() => {
    if (
      rightSource.connection &&
      !rightSource.sessionId &&
      !rightSource.connecting &&
      !rightSource.connectionError
    ) {
      connectSource('right', rightSource.connection).catch(() => undefined);
    }
  }, [
    rightSource.connection,
    rightSource.sessionId,
    rightSource.connecting,
    rightSource.connectionError,
    connectSource,
  ]);

  const sourceProjectRef = useRef(projectId);
  useEffect(() => {
    const sharedSessions = sharedSessionsRef.current;
    if (sourceProjectRef.current !== projectId) {
      sourceProjectRef.current = projectId;
      const empty: DiffSourceState = {
        mode: 'table',
        loading: false,
        connecting: false,
        namespacesLoading: false,
      };
      setLeftSource(empty);
      setRightSource(empty);
      setDiffResult(null);
      setKeyColumns([]);
      setLeftTableSchema(null);
      setRightTableSchema(null);
      tableSchemaCacheRef.current.clear();
    }
    return () => {
      leftConnectAttemptRef.current += 1;
      rightConnectAttemptRef.current += 1;
      leftExecAttemptRef.current += 1;
      rightExecAttemptRef.current += 1;
      for (const [key, entry] of sharedSessions) {
        if (entry.projectId !== projectId) continue;
        sharedSessions.delete(key);
        entry.promise.then(disconnect).catch(err => {
          console.warn('Failed to disconnect diff session', err);
        });
      }
    };
  }, [projectId, setKeyColumns]);

  const executeSource = useCallback(
    async (
      source: DiffSourceState,
      updateFn: (updates: Partial<DiffSourceState>) => void,
      attemptRef: MutableRefObject<number>
    ) => {
      if (source.mode !== 'snapshot' && (!source.sessionId || !source.namespace)) return;
      if (source.mode === 'snapshot' && !source.snapshotId && source.result) {
        return {
          result: source.result,
          truncated: source.truncated ?? false,
          attemptId: attemptRef.current,
        };
      }

      attemptRef.current += 1;
      const attemptId = attemptRef.current;

      updateFn({ loading: true, result: undefined, truncated: undefined, error: undefined });

      try {
        let response: Awaited<ReturnType<typeof executeQuery>>;
        const executionLimit = rowLimitRef.current;

        if (source.mode === 'snapshot' && source.snapshotId) {
          response = await getSnapshot(source.snapshotId);
        } else if (
          source.mode === 'table' &&
          source.tableName &&
          source.sessionId &&
          source.namespace
        ) {
          response = await previewTable(
            source.sessionId,
            source.namespace,
            source.tableName,
            executionLimit
          );
        } else if (
          source.mode === 'query' &&
          source.query?.trim() &&
          source.sessionId &&
          source.namespace
        ) {
          response = await executeQuery(source.sessionId, source.query, {
            namespace: source.namespace,
          });
        } else {
          if (attemptRef.current === attemptId) updateFn({ loading: false });
          return;
        }

        if (attemptRef.current !== attemptId) return;
        if (!response.success || !response.result)
          throw new Error(response.error || i18n.t('common.unknownError'));
        const result = response.result;
        const truncated =
          source.mode === 'table'
            ? result.rows.length >= executionLimit
            : (response.truncated ?? source.truncated ?? false);
        updateFn({ result, truncated, loading: false });
        return { result, truncated, attemptId };
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

  useEffect(() => {
    if (
      leftSource.mode !== 'snapshot' ||
      !leftSource.snapshotId ||
      leftSource.result ||
      leftSource.loading ||
      leftSource.error
    )
      return;
    executeLeft().catch(() => undefined);
  }, [
    leftSource.mode,
    leftSource.snapshotId,
    leftSource.result,
    leftSource.loading,
    leftSource.error,
    executeLeft,
  ]);

  useEffect(() => {
    if (
      rightSource.mode !== 'snapshot' ||
      !rightSource.snapshotId ||
      rightSource.result ||
      rightSource.loading ||
      rightSource.error
    )
      return;
    executeRight().catch(() => undefined);
  }, [
    rightSource.mode,
    rightSource.snapshotId,
    rightSource.result,
    rightSource.loading,
    rightSource.error,
    executeRight,
  ]);

  useEffect(() => {
    if (
      leftSource.mode !== 'table' ||
      !leftSource.sessionId ||
      !leftSource.namespace ||
      !leftSource.tableName
    ) {
      setLeftTableSchema(null);
      return;
    }

    const cacheKey = getTableSchemaCacheKey(
      leftSource.sessionId,
      leftSource.namespace,
      leftSource.tableName
    );
    const cachedSchema = tableSchemaCacheRef.current.get(cacheKey);
    if (cachedSchema !== undefined) {
      setLeftTableSchema(cachedSchema);
      return;
    }

    let cancelled = false;
    describeTable(leftSource.sessionId, leftSource.namespace, leftSource.tableName)
      .then(response => {
        const schema = response.success ? (response.schema ?? null) : null;
        tableSchemaCacheRef.current.set(cacheKey, schema);
        if (!cancelled) {
          setLeftTableSchema(schema);
        }
      })
      .catch(() => {
        tableSchemaCacheRef.current.set(cacheKey, null);
        if (!cancelled) {
          setLeftTableSchema(null);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [leftSource.mode, leftSource.sessionId, leftSource.namespace, leftSource.tableName]);

  useEffect(() => {
    if (
      rightSource.mode !== 'table' ||
      !rightSource.sessionId ||
      !rightSource.namespace ||
      !rightSource.tableName
    ) {
      setRightTableSchema(null);
      return;
    }

    const cacheKey = getTableSchemaCacheKey(
      rightSource.sessionId,
      rightSource.namespace,
      rightSource.tableName
    );
    const cachedSchema = tableSchemaCacheRef.current.get(cacheKey);
    if (cachedSchema !== undefined) {
      setRightTableSchema(cachedSchema);
      return;
    }

    let cancelled = false;
    describeTable(rightSource.sessionId, rightSource.namespace, rightSource.tableName)
      .then(response => {
        const schema = response.success ? (response.schema ?? null) : null;
        tableSchemaCacheRef.current.set(cacheKey, schema);
        if (!cancelled) {
          setRightTableSchema(schema);
        }
      })
      .catch(() => {
        tableSchemaCacheRef.current.set(cacheKey, null);
        if (!cancelled) {
          setRightTableSchema(null);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [rightSource.mode, rightSource.sessionId, rightSource.namespace, rightSource.tableName]);

  const commonColumns = useMemo(() => {
    if (!leftSource.result || !rightSource.result) return [];
    return findCommonColumns(leftSource.result, rightSource.result);
  }, [leftSource.result, rightSource.result]);

  const leftColumnMetadata = useMemo(
    () => buildColumnMetadataMap(leftTableSchema),
    [leftTableSchema]
  );
  const rightColumnMetadata = useMemo(
    () => buildColumnMetadataMap(rightTableSchema),
    [rightTableSchema]
  );

  useEffect(() => {
    if (!leftSource.result || !rightSource.result) return;
    const commonNames = new Set(commonColumns.map(col => col.name));
    const previous = keyColumnsRef.current;
    const next = previous.filter(name => commonNames.has(name));
    if (next.length !== previous.length) setKeyColumns(next);
  }, [leftSource.result, rightSource.result, commonColumns, setKeyColumns]);

  const compareBlockedReason = useMemo(() => {
    if (!leftSource.result || !rightSource.result) return 'missingResults';
    return null;
  }, [leftSource.result, rightSource.result]);

  const trivialCommonColumns = useMemo(() => {
    if (!leftSource.result || !rightSource.result) return [];

    const leftColumnsByName = new Map(
      leftSource.result.columns.map(column => [column.name, column])
    );
    const rightColumnsByName = new Map(
      rightSource.result.columns.map(column => [column.name, column])
    );

    return commonColumns
      .filter(column => {
        const leftColumn = leftColumnsByName.get(column.name);
        const rightColumn = rightColumnsByName.get(column.name);
        if (!leftColumn || !rightColumn) return false;

        return (
          isTrivialColumn(leftColumn, leftColumnMetadata.get(column.name)) &&
          isTrivialColumn(rightColumn, rightColumnMetadata.get(column.name))
        );
      })
      .map(column => column.name);
  }, [
    commonColumns,
    leftSource.result,
    rightSource.result,
    leftColumnMetadata,
    rightColumnMetadata,
  ]);

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
        keyColumns.length > 0 ? keyColumns : undefined,
        { truncated: Boolean(leftSource.truncated || rightSource.truncated) }
      );
      setDiffResult(result);
    } finally {
      setComparing(false);
    }
  }, [
    leftSource.result,
    rightSource.result,
    leftSource.truncated,
    rightSource.truncated,
    keyColumns,
    compareBlockedReason,
  ]);

  const swap = useCallback(() => {
    leftExecAttemptRef.current += 1;
    rightExecAttemptRef.current += 1;
    leftConnectAttemptRef.current += 1;
    rightConnectAttemptRef.current += 1;
    setLeftSource({ ...rightSource, loading: false, connecting: false, namespacesLoading: false });
    setRightSource({ ...leftSource, loading: false, connecting: false, namespacesLoading: false });
    setDiffResult(null);
  }, [leftSource, rightSource]);

  const refresh = useCallback(async () => {
    const [left, right] = await Promise.all([
      executeSource(leftSource, updateLeftSource, leftExecAttemptRef),
      executeSource(rightSource, updateRightSource, rightExecAttemptRef),
    ]);
    if (
      diffResult &&
      left &&
      right &&
      left.attemptId === leftExecAttemptRef.current &&
      right.attemptId === rightExecAttemptRef.current
    ) {
      setDiffResult(
        compareResults(
          left.result,
          right.result,
          keyColumnsRef.current.length ? keyColumnsRef.current : undefined,
          { truncated: left.truncated || right.truncated }
        )
      );
    }
  }, [executeSource, leftSource, rightSource, updateLeftSource, updateRightSource, diffResult]);

  const reset = useCallback(() => {
    leftExecAttemptRef.current += 1;
    rightExecAttemptRef.current += 1;
    leftConnectAttemptRef.current += 1;
    rightConnectAttemptRef.current += 1;
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
  }, [leftSource.connectionId, rightSource.connectionId, releaseConnection, setKeyColumns]);

  const canCompare = useMemo(() => compareBlockedReason === null, [compareBlockedReason]);

  const hasResults = useMemo(
    () => Boolean(leftSource.result || rightSource.result),
    [leftSource.result, rightSource.result]
  );

  const truncatedSides = (['left', 'right'] as const).filter(side => {
    const source = side === 'left' ? leftSource : rightSource;
    return source.truncated;
  });

  return {
    rowLimit,
    setRowLimit,
    truncatedSides,
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
