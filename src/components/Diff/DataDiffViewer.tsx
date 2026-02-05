/**
 * DataDiffViewer - Visual comparison of two data sources
 *
 * Allows comparing two query results or table contents side by side,
 * highlighting differences like a Git diff viewer.
 */
import { useState, useMemo, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { DiffSource } from '@/lib/tabs';
import { Namespace, SavedConnection, listSavedConnections } from '@/lib/tauri';
import { UI_EVENT_CONNECTIONS_CHANGED } from '@/lib/uiEvents';
import { DiffToolbar } from './DiffToolbar';
import { DiffSourcePanel } from './DiffSourcePanel';
import { DiffConfigPanel } from './DiffConfigPanel';
import { DiffStatsBar, DiffFilter } from './DiffStatsBar';
import { DiffResultsGrid } from './DiffResultsGrid';
import { useDiffSources } from './hooks/useDiffSources';

const DEFAULT_PROJECT = 'default';

interface DataDiffViewerProps {
  activeConnection?: SavedConnection | null;
  namespace?: Namespace;
  leftSource?: DiffSource;
  rightSource?: DiffSource;
  onSourceChange?: (side: 'left' | 'right', source: DiffSource) => void;
}

export function DataDiffViewer({
  activeConnection,
  namespace,
  leftSource: initialLeftSource,
  rightSource: initialRightSource,
  onSourceChange,
}: DataDiffViewerProps) {
  const { t } = useTranslation();

  const [connections, setConnections] = useState<SavedConnection[]>([]);
  const [connectionsLoading, setConnectionsLoading] = useState(false);

  const loadConnections = useCallback(async () => {
    setConnectionsLoading(true);
    try {
      const saved = await listSavedConnections(DEFAULT_PROJECT);
      setConnections(saved);
    } catch (err) {
      console.warn('Failed to load saved connections', err);
      setConnections([]);
    } finally {
      setConnectionsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadConnections();
  }, [loadConnections]);

  useEffect(() => {
    const handler = () => loadConnections();
    window.addEventListener(UI_EVENT_CONNECTIONS_CHANGED, handler);
    return () => window.removeEventListener(UI_EVENT_CONNECTIONS_CHANGED, handler);
  }, [loadConnections]);

  const connectionsById = useMemo(
    () => new Map(connections.map((conn) => [conn.id, conn])),
    [connections]
  );

  const {
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
    keyColumns,
    setKeyColumns,
    compare,
    diffResult,
    comparing,
    swap,
    refresh,
    canCompare,
    hasResults,
    commonColumns,
    trivialCommonColumns,
    compareBlockedReason,
    compareWarning,
  } = useDiffSources({
    activeConnection,
    initialNamespace: namespace,
    initialLeftSource,
    initialRightSource,
  });

  useEffect(() => {
    if (leftSource.connectionId && !leftSource.connection) {
      const connection = connectionsById.get(leftSource.connectionId) ?? null;
      if (connection) {
        setLeftConnection(connection).catch(() => undefined);
      }
    }
  }, [leftSource.connectionId, leftSource.connection, connectionsById, setLeftConnection]);

  useEffect(() => {
    if (rightSource.connectionId && !rightSource.connection) {
      const connection = connectionsById.get(rightSource.connectionId) ?? null;
      if (connection) {
        setRightConnection(connection).catch(() => undefined);
      }
    }
  }, [rightSource.connectionId, rightSource.connection, connectionsById, setRightConnection]);

  const handleLeftConnectionChange = useCallback(
    (connectionId: string | null) => {
      const connection = connectionId ? connectionsById.get(connectionId) ?? null : null;
      setLeftConnection(connection).catch(() => undefined);
    },
    [connectionsById, setLeftConnection]
  );

  const handleRightConnectionChange = useCallback(
    (connectionId: string | null) => {
      const connection = connectionId ? connectionsById.get(connectionId) ?? null : null;
      setRightConnection(connection).catch(() => undefined);
    },
    [connectionsById, setRightConnection]
  );

  const compareWarningText = useMemo(() => {
    if (compareWarning === 'noCommonColumns') {
      return t('diff.noCommonColumnsWarning');
    }
    if (compareWarning !== 'trivialCommonColumns' || commonColumns.length === 0) return null;
    const columns =
      trivialCommonColumns.length > 0
        ? trivialCommonColumns.join(', ')
        : commonColumns.map((col) => col.name).join(', ');
    return t('diff.trivialCommonColumnsWarning', { columns });
  }, [compareWarning, commonColumns, trivialCommonColumns, t]);

  const compareBlockedText = useMemo(() => {
    if (compareBlockedReason === 'missingResults') {
      return null;
    }
    return null;
  }, [compareBlockedReason]);

  // UI state
  const [filter, setFilter] = useState<DiffFilter>('all');
  const [showUnchanged, setShowUnchanged] = useState(true);

  // Sync sources with parent when they change
  useEffect(() => {
    if (onSourceChange && leftSource.result) {
      onSourceChange('left', {
        type: leftSource.mode,
        label: leftSource.tableName ?? t('diff.leftSource'),
        tableName: leftSource.tableName,
        query: leftSource.query,
        result: leftSource.result,
        namespace: leftSource.namespace,
        connectionId: leftSource.connectionId,
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [leftSource.result]);

  useEffect(() => {
    if (onSourceChange && rightSource.result) {
      onSourceChange('right', {
        type: rightSource.mode,
        label: rightSource.tableName ?? t('diff.rightSource'),
        tableName: rightSource.tableName,
        query: rightSource.query,
        result: rightSource.result,
        namespace: rightSource.namespace,
        connectionId: rightSource.connectionId,
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rightSource.result]);

  // Filter rows based on current filter and showUnchanged setting
  const filteredRows = useMemo(() => {
    if (!diffResult) return [];

    let rows = diffResult.rows;

    // Filter by status
    if (filter !== 'all') {
      rows = rows.filter((r) => r.status === filter);
    }

    // Filter out unchanged if not showing
    if (!showUnchanged && filter === 'all') {
      rows = rows.filter((r) => r.status !== 'unchanged');
    }

    return rows;
  }, [diffResult, filter, showUnchanged]);

  // Get columns from results for config panel
  const leftColumns = leftSource.result?.columns;
  const rightColumns = rightSource.result?.columns;

  return (
    <div className="flex flex-col h-full bg-background">
      {/* Toolbar */}
      <DiffToolbar
        onSwap={swap}
        onRefresh={refresh}
        diffResult={diffResult}
        canSwap={hasResults}
        canRefresh={hasResults}
      />

      {/* Source panels */}
      <div className="grid grid-cols-2 gap-4 p-4 border-b border-border">
        <DiffSourcePanel
          label={t('diff.leftSource')}
          connections={connections}
          connectionsLoading={connectionsLoading}
          source={leftSource}
          onConnectionChange={handleLeftConnectionChange}
          onNamespaceChange={setLeftNamespace}
          onSourceChange={updateLeftSource}
          onExecute={executeLeft}
        />
        <DiffSourcePanel
          label={t('diff.rightSource')}
          connections={connections}
          connectionsLoading={connectionsLoading}
          source={rightSource}
          onConnectionChange={handleRightConnectionChange}
          onNamespaceChange={setRightNamespace}
          onSourceChange={updateRightSource}
          onExecute={executeRight}
        />
      </div>

      {/* Config panel */}
      <div className="px-4 py-3 border-b border-border">
        <DiffConfigPanel
          leftColumns={leftColumns}
          rightColumns={rightColumns}
          keyColumns={keyColumns}
          onKeyColumnsChange={setKeyColumns}
          onCompare={compare}
          comparing={comparing}
          canCompare={canCompare}
          compareBlockedText={compareBlockedText}
          compareWarningText={compareWarningText}
          leftSessionId={leftSource.sessionId}
          rightSessionId={rightSource.sessionId}
          leftNamespace={leftSource.namespace}
          rightNamespace={rightSource.namespace}
          leftTableName={leftSource.tableName}
          rightTableName={rightSource.tableName}
        />
      </div>

      {/* Stats bar - only show when there's a diff result */}
      {diffResult && (
        <DiffStatsBar
          stats={diffResult.stats}
          filter={filter}
          onFilterChange={setFilter}
          showUnchanged={showUnchanged}
          onShowUnchangedChange={setShowUnchanged}
        />
      )}

      {/* Results grid */}
      <div className="flex-1 overflow-hidden">
        <DiffResultsGrid diffResult={diffResult} filteredRows={filteredRows} />
      </div>
    </div>
  );
}
