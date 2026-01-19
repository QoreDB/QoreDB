import { useState, useCallback, useEffect, useRef, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { MONGO_TEMPLATES } from '../Editor/MongoEditor';
import { DocumentEditorModal } from '../Editor/DocumentEditorModal';
import { QueryHistory } from '../History/QueryHistory';
import { executeQuery, cancelQuery, QueryResult, Environment, Value, Namespace } from '../../lib/tauri';
import { addToHistory } from '../../lib/history';
import { logError } from '../../lib/errorLog';
import { ENVIRONMENT_CONFIG, getDangerousQueryTarget, isDangerousQuery, isDropDatabaseQuery, isMutationQuery } from '../../lib/environment';
import { Driver } from '../../lib/drivers';
import { ProductionConfirmDialog } from '../Guard/ProductionConfirmDialog';
import { DangerConfirmDialog } from '../Guard/DangerConfirmDialog';
import { toast } from 'sonner';
import { forceRefreshCache } from '../../hooks/useSchemaCache';
import { QueryPanelToolbar } from './QueryPanelToolbar';
import { QueryPanelEditor } from './QueryPanelEditor';
import { QueryPanelResults, QueryResultEntry } from './QueryPanelResults';
import { getCollectionFromQuery, getDefaultQuery, shouldRefreshSchema } from './queryPanelUtils';
import { formatSql } from '../../lib/sqlFormatter';
import { SQLEditorHandle } from '../Editor/SQLEditor';
import { SaveQueryDialog } from './SaveQueryDialog';
import { QueryLibraryModal } from './QueryLibraryModal';

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

interface QueryPanelProps {
	sessionId: string | null;
	dialect?: Driver;
	environment?: Environment;
	readOnly?: boolean;
	connectionName?: string;
	connectionDatabase?: string;
	activeNamespace?: Namespace | null;
	initialQuery?: string;
	onSchemaChange?: () => void;
  onOpenLibrary?: () => void;
  isActive?: boolean;
}

export function QueryPanel({
  sessionId,
  dialect = Driver.Postgres,
  environment = 'development',
  readOnly = false,
  connectionName,
  connectionDatabase,
  activeNamespace,
  initialQuery,
  onSchemaChange,
  onOpenLibrary,
  isActive = true,
}: QueryPanelProps) {
  const { t } = useTranslation();
  const isMongo = dialect === Driver.Mongodb;
  const defaultQuery = getDefaultQuery(isMongo);

  const [query, setQuery] = useState(initialQuery || defaultQuery);
  const [results, setResults] = useState<QueryResultEntry[]>([]);
  const [activeResultId, setActiveResultId] = useState<string | null>(null);
  const [keepResults, setKeepResults] = useState(true);
  const [loading, setLoading] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [activeQueryId, setActiveQueryId] = useState<string | null>(null);
  const [panelError, setPanelError] = useState<string | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [dangerConfirmOpen, setDangerConfirmOpen] = useState(false);
  const [dangerConfirmLabel, setDangerConfirmLabel] = useState<string | undefined>(undefined);
  const [dangerConfirmInfo, setDangerConfirmInfo] = useState<string | undefined>(undefined);
  const [pendingQuery, setPendingQuery] = useState<string | null>(null);
  const sqlEditorRef = useRef<SQLEditorHandle>(null);
  const [saveDialogOpen, setSaveDialogOpen] = useState(false);
  const [libraryOpen, setLibraryOpen] = useState(false);
  const [queryToSave, setQueryToSave] = useState<string>('');

  const isExplainSupported = useMemo(() => dialect === Driver.Postgres, [dialect]);

  // Document Modal State
  const [docModalOpen, setDocModalOpen] = useState(false);
  const [docModalMode, setDocModalMode] = useState<'insert' | 'edit'>('insert');
  const [docModalData, setDocModalData] = useState('{}'); // JSON string
  const [docOriginalId, setDocOriginalId] = useState<Value | undefined>(undefined);
  const collectionName = getCollectionFromQuery(query);

  useEffect(() => {
    if (initialQuery) {
      setQuery(initialQuery);
      setResults([]);
      setActiveResultId(null);
      setPanelError(null);
    }
  }, [initialQuery]);

  const envConfig = ENVIRONMENT_CONFIG[environment];

  const runQuery = useCallback(
    async (
      queryToRun: string,
      acknowledgedDangerous = false,
      kind: QueryResultEntry['kind'] = 'query'
    ) => {
      if (!sessionId) {
        setPanelError(t('query.noConnectionError'));
        return;
      }

      setLoading(true);
      setPanelError(null);

      const queryId =
        crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
      setActiveQueryId(queryId);

      const startTime = performance.now();
      try {
        const response = await executeQuery(sessionId, queryToRun, {
          acknowledgedDangerous,
          queryId,
        });
        const endTime = performance.now();
        const totalTime = endTime - startTime;

        if (response.success && response.result) {
          const enrichedResult: QueryResult = {
            ...response.result,
            total_time_ms: totalTime,
          } as QueryResult & { total_time_ms: number };

          const entry: QueryResultEntry = {
            id: queryId,
            kind,
            query: queryToRun,
            result: enrichedResult,
            executedAt: Date.now(),
            totalTimeMs: totalTime,
            executionTimeMs: response.result.execution_time_ms,
            rowCount: response.result.rows.length,
          };
          setResults(prev => {
            const next = keepResults ? [...prev, entry] : [entry];
            if (next.length > 12) {
              return next.slice(next.length - 12);
            }
            return next;
          });
          setActiveResultId(queryId);

          addToHistory({
            query: queryToRun,
            sessionId,
            driver: dialect,
            executedAt: Date.now(),
            executionTimeMs: response.result.execution_time_ms,
            totalTimeMs: totalTime,
            rowCount: response.result.rows.length,
          });

          if (shouldRefreshSchema(queryToRun, isMongo)) {
            forceRefreshCache(sessionId);
            onSchemaChange?.();
          }
        } else {
          const entry: QueryResultEntry = {
            id: queryId,
            kind,
            query: queryToRun,
            error: response.error || t('query.queryFailed'),
            executedAt: Date.now(),
          };
          setResults(prev => {
            const next = keepResults ? [...prev, entry] : [entry];
            if (next.length > 12) {
              return next.slice(next.length - 12);
            }
            return next;
          });
          setActiveResultId(queryId);
          addToHistory({
            query: queryToRun,
            sessionId,
            driver: dialect,
            executedAt: Date.now(),
            executionTimeMs: 0,
            totalTimeMs: totalTime,
            error: response.error || t('query.queryFailed'),
          });
          logError('QueryPanel', response.error || t('query.queryFailed'), queryToRun, sessionId);
        }
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : t('common.error');
        const entry: QueryResultEntry = {
          id: queryId,
          kind,
          query: queryToRun,
          error: errorMessage,
          executedAt: Date.now(),
        };
        setResults(prev => {
          const next = keepResults ? [...prev, entry] : [entry];
          if (next.length > 12) {
            return next.slice(next.length - 12);
          }
          return next;
        });
        setActiveResultId(queryId);
        logError('QueryPanel', errorMessage, queryToRun, sessionId || undefined);
      } finally {
        setLoading(false);
        setActiveQueryId(null);
      }
    },
    [sessionId, dialect, t, onSchemaChange, isMongo, keepResults]
  );

  const handleExecute = useCallback(
    async (queryText?: string) => {
      if (!sessionId) {
        setPanelError(t('query.noConnectionError'));
        return;
      }

      const queryToRun = queryText || query;
      if (!queryToRun.trim()) return;

      const isMutation = isMutationQuery(queryToRun, isMongo ? 'mongodb' : 'sql');

      if (readOnly && isMutation) {
        toast.error(t('environment.blocked'));
        return;
      }

      const isDangerous = !isMongo && isDangerousQuery(queryToRun);
      if (isDangerous) {
        const fallbackLabel = (connectionDatabase || connectionName || 'PROD').trim() || 'PROD';
        const target = getDangerousQueryTarget(queryToRun);
        const isDropDatabase = !isMongo && isDropDatabaseQuery(queryToRun);
        const requiresTyping = environment === 'production' || isDropDatabase;
        const warningInfoParts = [];
        if (target) {
          warningInfoParts.push(t('environment.dangerousQueryTarget', { target }));
        }
        if (environment === 'production') {
          warningInfoParts.push(t('environment.prodWarning'));
        }
        setPendingQuery(queryToRun);
        setDangerConfirmLabel(requiresTyping ? target || fallbackLabel : undefined);
        setDangerConfirmInfo(warningInfoParts.length ? warningInfoParts.join(' | ') : undefined);
        setDangerConfirmOpen(true);
        return;
      }

      if (environment === 'production' && isMutation) {
        setPendingQuery(queryToRun);
        setConfirmOpen(true);
        return;
      }

      await runQuery(queryToRun, false, 'query');
    },
    [
      sessionId,
      query,
      isMongo,
      readOnly,
      environment,
      t,
      runQuery,
      connectionDatabase,
      connectionName,
    ]
  );

  const handleConfirm = useCallback(async () => {
    if (!pendingQuery) {
      setConfirmOpen(false);
      return;
    }

    const queryToRun = pendingQuery;
    setPendingQuery(null);
    setConfirmOpen(false);
    await runQuery(queryToRun, false, 'query');
  }, [pendingQuery, runQuery]);

  const handleDangerConfirm = useCallback(async () => {
    if (!pendingQuery) {
      setDangerConfirmOpen(false);
      return;
    }

    const queryToRun = pendingQuery;
    setPendingQuery(null);
    setDangerConfirmOpen(false);
    setDangerConfirmInfo(undefined);
    setDangerConfirmLabel(undefined);
    await runQuery(queryToRun, true, 'query');
  }, [pendingQuery, runQuery]);

  const handleCancel = useCallback(async () => {
    if (!sessionId || !loading) return;

    setCancelling(true);
    try {
      await cancelQuery(sessionId, activeQueryId ?? undefined);
    } catch (err) {
      console.error('Failed to cancel:', err);
    } finally {
      setCancelling(false);
      setLoading(false);
    }
  }, [sessionId, loading, activeQueryId]);

  const handleEditDocument = useCallback(
    (doc: Record<string, unknown>, idValue?: Value) => {
      if (!isMongo) return;
      setDocModalMode('edit');
      setDocModalData(JSON.stringify(doc, null, 2));
      setDocOriginalId(idValue);
      setDocModalOpen(true);
    },
    [isMongo]
  );

  const handleNewDocument = () => {
    setDocModalMode('insert');
    setDocModalData('{\n  \n}');
    setDocOriginalId(undefined);
    setDocModalOpen(true);
  };

  const handleTemplateSelect = useCallback((templateKey: keyof typeof MONGO_TEMPLATES) => {
    setQuery(prev => MONGO_TEMPLATES[templateKey] ?? prev);
  }, []);

  const handleFormat = useCallback(() => {
    if (isMongo) return;
    const formatted = formatSql(query, dialect);
    setQuery(formatted);
  }, [dialect, isMongo, query]);

  const handleExplain = useCallback(async () => {
    if (!sessionId || isMongo || !isExplainSupported) {
      return;
    }
    const selection = sqlEditorRef.current?.getSelection();
    const queryToExplain = selection && selection.trim().length > 0 ? selection : query;
    if (!queryToExplain.trim()) return;
    const trimmed = queryToExplain.replace(/;+\s*$/, '');
    const explainQuery = `EXPLAIN (FORMAT JSON) ${trimmed}`;
    await runQuery(explainQuery, false, 'explain');
  }, [sessionId, isMongo, isExplainSupported, query, runQuery]);

  const handleToggleKeepResults = useCallback(() => {
    setKeepResults(prev => {
      if (prev) {
        setResults(current => {
          const active = current.find(entry => entry.id === activeResultId);
          return active ? [active] : [];
        });
      }
      return !prev;
    });
  }, [activeResultId]);

  const handleExecuteCurrent = useCallback(() => handleExecute(), [handleExecute]);
  const handleExecuteSelection = useCallback(
    (selection: string) => handleExecute(selection),
    [handleExecute]
  );

  const runCurrentQuery = useCallback(() => handleExecute(), [handleExecute]);

  const handleSaveToLibrary = useCallback(() => {
    const selection = !isMongo ? sqlEditorRef.current?.getSelection() : '';
    const candidate = selection && selection.trim().length > 0 ? selection : query;
    setQueryToSave(candidate);
    setSaveDialogOpen(true);
  }, [isMongo, query]);

  useEffect(() => {
    if (!isActive) return;

    function handleKeyDown(e: KeyboardEvent) {
      if (isTextInputTarget(e.target)) return;
      if (saveDialogOpen || historyOpen || libraryOpen || confirmOpen || dangerConfirmOpen) return;

      // Cmd/Ctrl+S: Save query to library
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key.toLowerCase() === 's') {
        e.preventDefault();
        handleSaveToLibrary();
        return;
      }

      // Cmd/Ctrl+Shift+H: Open query history
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'h') {
        e.preventDefault();
        setHistoryOpen(true);
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [
    confirmOpen,
    dangerConfirmOpen,
    handleSaveToLibrary,
    historyOpen,
    isActive,
    libraryOpen,
    saveDialogOpen,
  ]);

  return (
    <div className="flex flex-col flex-1 bg-background rounded-lg border border-border shadow-sm overflow-hidden">
      <QueryPanelToolbar
        loading={loading}
        cancelling={cancelling}
        sessionId={sessionId}
        environment={environment}
        envConfig={envConfig}
        readOnly={readOnly}
        isMongo={isMongo}
        keepResults={keepResults}
        isExplainSupported={isExplainSupported}
        onExecute={handleExecuteCurrent}
        onCancel={handleCancel}
        onExplain={handleExplain}
        onToggleKeepResults={handleToggleKeepResults}
        onNewDocument={handleNewDocument}
        onHistoryOpen={() => setHistoryOpen(true)}
        onLibraryOpen={() => (onOpenLibrary ? onOpenLibrary() : setLibraryOpen(true))}
        onSaveToLibrary={handleSaveToLibrary}
        onTemplateSelect={handleTemplateSelect}
      />

      <QueryPanelEditor
        isMongo={isMongo}
        query={query}
        loading={loading}
        dialect={dialect}
        sessionId={sessionId}
        connectionDatabase={connectionDatabase}
        activeNamespace={activeNamespace}
        onQueryChange={setQuery}
        onExecute={handleExecuteCurrent}
        onExecuteSelection={handleExecuteSelection}
        onFormat={handleFormat}
        sqlEditorRef={sqlEditorRef}
      />

      <QueryPanelResults
        panelError={panelError}
        results={results}
        activeResultId={activeResultId}
        isMongo={isMongo}
        sessionId={sessionId}
        connectionName={connectionName}
        connectionDatabase={connectionDatabase}
        environment={environment}
        readOnly={readOnly}
        query={query}
        onSelectResult={setActiveResultId}
        onCloseResult={(resultId: string) => {
          setResults(prev => {
            const next = prev.filter(entry => entry.id !== resultId);
            if (activeResultId === resultId) {
              const fallback = next[next.length - 1];
              setActiveResultId(fallback?.id || null);
            }
            return next;
          });
        }}
        onRowsDeleted={runCurrentQuery}
        onEditDocument={handleEditDocument}
      />

      {/* History Modal */}
      <QueryHistory
        isOpen={historyOpen}
        onClose={() => setHistoryOpen(false)}
        onSelectQuery={setQuery}
        sessionId={sessionId || undefined}
      />

      <ProductionConfirmDialog
        open={confirmOpen}
        onOpenChange={open => {
          setConfirmOpen(open);
          if (!open) {
            setPendingQuery(null);
          }
        }}
        title={t('environment.confirmTitle')}
        confirmationLabel={(connectionDatabase || connectionName || 'PROD').trim() || 'PROD'}
        confirmLabel={t('common.confirm')}
        onConfirm={handleConfirm}
      />

      <DangerConfirmDialog
        open={dangerConfirmOpen}
        onOpenChange={open => {
          setDangerConfirmOpen(open);
          if (!open) {
            setPendingQuery(null);
            setDangerConfirmInfo(undefined);
            setDangerConfirmLabel(undefined);
          }
        }}
        title={t('environment.dangerousQueryTitle')}
        description={t('environment.dangerousQuery')}
        warningInfo={dangerConfirmInfo}
        confirmationLabel={dangerConfirmLabel}
        confirmLabel={t('common.confirm')}
        onConfirm={handleDangerConfirm}
      />

      <DocumentEditorModal
        isOpen={docModalOpen}
        onClose={() => setDocModalOpen(false)}
        mode={docModalMode}
        initialData={docModalData}
        sessionId={sessionId || ''}
        database={connectionDatabase || 'admin'}
        collection={collectionName}
        originalId={docOriginalId}
        onSuccess={() => {
          handleExecuteCurrent();
        }}
        readOnly={readOnly}
      />

      <SaveQueryDialog
        open={saveDialogOpen}
        onOpenChange={setSaveDialogOpen}
        initialQuery={queryToSave || query}
        driver={dialect}
        database={connectionDatabase}
        onSaved={() => {
          // no-op for now (modal pulls from storage)
        }}
      />

      <QueryLibraryModal
        isOpen={libraryOpen}
        onClose={() => setLibraryOpen(false)}
        onSelectQuery={q => setQuery(q)}
      />
    </div>
  );
}
