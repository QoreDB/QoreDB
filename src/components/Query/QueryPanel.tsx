import { useState, useCallback, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { MONGO_TEMPLATES } from '../Editor/MongoEditor';
import { DocumentEditorModal } from '../Editor/DocumentEditorModal';
import { QueryHistory } from '../History/QueryHistory';
import { executeQuery, cancelQuery, QueryResult, Environment, Value } from '../../lib/tauri';
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
import { QueryPanelResults } from './QueryPanelResults';
import { getCollectionFromQuery, getDefaultQuery, shouldRefreshSchema } from './queryPanelUtils';

interface QueryPanelProps {
	sessionId: string | null;
	dialect?: Driver;
	environment?: Environment;
	readOnly?: boolean;
	connectionName?: string;
	connectionDatabase?: string;
	initialQuery?: string;
	onSchemaChange?: () => void;
}

export function QueryPanel({
  sessionId,
  dialect = Driver.Postgres,
  environment = 'development',
  readOnly = false,
  connectionName,
  connectionDatabase,
  initialQuery,
  onSchemaChange,
}: QueryPanelProps) {
  const { t } = useTranslation();
  const isMongo = dialect === Driver.Mongodb;
  const defaultQuery = getDefaultQuery(isMongo);

  const [query, setQuery] = useState(initialQuery || defaultQuery);
  const [result, setResult] = useState<QueryResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [activeQueryId, setActiveQueryId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [dangerConfirmOpen, setDangerConfirmOpen] = useState(false);
  const [dangerConfirmLabel, setDangerConfirmLabel] = useState<string | undefined>(undefined);
  const [dangerConfirmInfo, setDangerConfirmInfo] = useState<string | undefined>(undefined);
  const [pendingQuery, setPendingQuery] = useState<string | null>(null);

  // Document Modal State
  const [docModalOpen, setDocModalOpen] = useState(false);
  const [docModalMode, setDocModalMode] = useState<'insert' | 'edit'>('insert');
  const [docModalData, setDocModalData] = useState('{}'); // JSON string
  const [docOriginalId, setDocOriginalId] = useState<Value | undefined>(undefined);
  const collectionName = getCollectionFromQuery(query);

  useEffect(() => {
    if (initialQuery) {
      setQuery(initialQuery);
    }
  }, [initialQuery]);

  const envConfig = ENVIRONMENT_CONFIG[environment];

  const runQuery = useCallback(
    async (queryToRun: string, acknowledgedDangerous = false) => {
      if (!sessionId) {
        setError(t('query.noConnectionError'));
        return;
      }

      setLoading(true);
      setError(null);
      setResult(null);

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
          const enrichedResult = {
            ...response.result,
            total_time_ms: totalTime,
          };
          setResult(enrichedResult);

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
          setError(response.error || t('query.queryFailed'));
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
        setError(errorMessage);
        logError('QueryPanel', errorMessage, queryToRun, sessionId || undefined);
      } finally {
        setLoading(false);
        setActiveQueryId(null);
      }
    },
    [sessionId, dialect, t, onSchemaChange, isMongo]
  );

  const handleExecute = useCallback(
    async (queryText?: string) => {
      if (!sessionId) {
        setError(t('query.noConnectionError'));
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

      await runQuery(queryToRun);
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
    await runQuery(queryToRun);
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
    await runQuery(queryToRun, true);
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

  const handleExecuteCurrent = useCallback(() => handleExecute(), [handleExecute]);
  const handleExecuteSelection = useCallback(
    (selection: string) => handleExecute(selection),
    [handleExecute]
  );

  const runCurrentQuery = useCallback(() => handleExecute(), [handleExecute]);

  return (
    <div className="flex flex-col h-full bg-background rounded-lg border border-border shadow-sm overflow-hidden">
      <QueryPanelToolbar
        loading={loading}
        cancelling={cancelling}
        sessionId={sessionId}
        environment={environment}
        envConfig={envConfig}
        readOnly={readOnly}
        isMongo={isMongo}
        onExecute={handleExecuteCurrent}
        onCancel={handleCancel}
        onNewDocument={handleNewDocument}
        onHistoryOpen={() => setHistoryOpen(true)}
        onTemplateSelect={handleTemplateSelect}
      />

      <QueryPanelEditor
        isMongo={isMongo}
        query={query}
        loading={loading}
        dialect={dialect}
        onQueryChange={setQuery}
        onExecute={handleExecuteCurrent}
        onExecuteSelection={handleExecuteSelection}
      />

      <QueryPanelResults
        error={error}
        result={result}
        isMongo={isMongo}
        sessionId={sessionId}
        connectionName={connectionName}
        connectionDatabase={connectionDatabase}
        environment={environment}
        readOnly={readOnly}
        query={query}
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
    </div>
  );
}

