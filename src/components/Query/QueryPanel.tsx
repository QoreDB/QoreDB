import { useState, useCallback, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { SQLEditor } from '../Editor/SQLEditor';
import { MongoEditor, MONGO_TEMPLATES } from '../Editor/MongoEditor';
import { DataGrid } from '../Grid/DataGrid';
import { DocumentResults } from '../Results/DocumentResults';
import { DocumentEditorModal } from '../Editor/DocumentEditorModal';
import { QueryHistory } from '../History/QueryHistory';
import { executeQuery, cancelQuery, QueryResult, Environment, Value } from '../../lib/tauri';
import { addToHistory } from '../../lib/history';
import { logError } from '../../lib/errorLog';
import { Button } from '@/components/ui/button';
import { Play, Square, AlertCircle, History, Shield, Lock, Plus } from 'lucide-react';
import { ENVIRONMENT_CONFIG, getDangerousQueryTarget, isDangerousQuery, isDropDatabaseQuery, isMutationQuery } from '../../lib/environment';
import { Driver } from '../../lib/drivers';
import { ProductionConfirmDialog } from '../Guard/ProductionConfirmDialog';
import { DangerConfirmDialog } from '../Guard/DangerConfirmDialog';
import { toast } from 'sonner';
import { forceRefreshCache } from '../../hooks/useSchemaCache';

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
	dialect = "postgres",
	environment = "development",
	readOnly = false,
	connectionName,
	connectionDatabase,
	initialQuery,
	onSchemaChange,
}: QueryPanelProps) {
	const { t } = useTranslation();
	const isMongo = dialect === "mongodb";
	const defaultQuery = isMongo ? MONGO_TEMPLATES.find : "SELECT 1;";

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
  const getCollectionFromQuery = (q: string) => {
    const trimmed = q.trim();
    if (trimmed.startsWith('{')) {
      try {
        const parsed = JSON.parse(trimmed);
        if (parsed && typeof parsed === 'object' && parsed.collection) {
          return String(parsed.collection);
        }
      } catch {
        return '';
      }
    }

    const directMatch = trimmed.match(/^db\.([a-zA-Z0-9_-]+)\./);
    if (directMatch) return directMatch[1];

    const getCollectionMatch = trimmed.match(/db\.getCollection\(['"]([^'"]+)['"]\)/);
    return getCollectionMatch ? getCollectionMatch[1] : '';
  };

  useEffect(() => {
    if (initialQuery) {
      setQuery(initialQuery);
    }
  }, [initialQuery]);

  const envConfig = ENVIRONMENT_CONFIG[environment];

  const shouldRefreshSchema = useCallback(
    (queryToCheck: string) => {
      if (!queryToCheck.trim()) return false;
      if (isMongo) {
        return (
          /\.createCollection\s*\(/i.test(queryToCheck) ||
          /\.dropDatabase\s*\(/i.test(queryToCheck) ||
          /\.drop\s*\(/i.test(queryToCheck) ||
          /\.renameCollection\s*\(/i.test(queryToCheck) ||
          /"operation"\s*:\s*"(create_collection|drop_collection|drop_database|rename_collection)"/i.test(
            queryToCheck
          )
        );
      }

      return /\b(CREATE|DROP|ALTER|TRUNCATE|RENAME)\b/i.test(queryToCheck);
    },
    [isMongo]
  );

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

          if (shouldRefreshSchema(queryToRun)) {
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
    [sessionId, dialect, t, onSchemaChange, shouldRefreshSchema]
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

    const handleEditDocument = useCallback((doc: Record<string, unknown>, idValue?: Value) => {
        if (!isMongo) return;
        setDocModalMode('edit');
        setDocModalData(JSON.stringify(doc, null, 2));
        setDocOriginalId(idValue);
        setDocModalOpen(true);
    }, [isMongo]);

    const handleNewDocument = () => {
        setDocModalMode('insert');
        setDocModalData('{\n  \n}');
        setDocOriginalId(undefined);
        setDocModalOpen(true);
    };

    const runCurrentQuery = () => handleExecute();

	return (
		<div className="flex flex-col h-full bg-background rounded-lg border border-border shadow-sm overflow-hidden">
			<div className="flex items-center gap-2 p-2 border-b border-border bg-muted/20">
				<Button
					onClick={() => handleExecute()}
					disabled={loading || !sessionId}
					className="w-24 gap-2"
				>
					{loading ? (
						<span className="flex items-center gap-2">{t("query.running")}</span>
					) : (
						<>
							<Play size={16} className="fill-current" /> {t("query.run")}
						</>
					)}
				</Button>

				{sessionId && environment !== "development" && (
					<span
						className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-bold rounded-full border"
						style={{
							backgroundColor: envConfig.bgSoft,
							color: envConfig.color,
							borderColor: envConfig.color,
						}}
					>
						<Shield size={12} />
						{envConfig.labelShort}
					</span>
				)}

				{sessionId && readOnly && (
					<span className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-bold rounded-full border border-warning/30 bg-warning/10 text-warning">
						<Lock size={12} />
						{t("environment.readOnly")}
					</span>
				)}

				{loading && (
					<Button
						variant="destructive"
						onClick={handleCancel}
						disabled={cancelling}
						className="w-24 gap-2"
					>
						<Square size={16} className="fill-current" /> {t("query.stop")}
					</Button>
				)}

                {/* New Document Button for MongoDB */}
                {isMongo && sessionId && (
                    <Button
                        variant="ghost"
                        size="sm"
                        className="h-9 px-2 text-muted-foreground hover:text-foreground ml-2"
                        onClick={handleNewDocument}
                        title={t("document.new")}
                    >
                        <Plus size={16} className="mr-1" />
                        <span className="hidden sm:inline">{t("document.new")}</span>
                    </Button>
                )}

				{isMongo && (
					<select
						className="h-9 rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
						onChange={(e) =>
							setQuery(
								MONGO_TEMPLATES[e.target.value as keyof typeof MONGO_TEMPLATES] || query
							)
						}
						defaultValue=""
					>
						<option value="" disabled>
							Templates...
						</option>
						<option value="find">find()</option>
						<option value="findOne">findOne()</option>
						<option value="aggregate">aggregate()</option>
						<option value="insertOne">insertOne()</option>
						<option value="updateOne">updateOne()</option>
						<option value="deleteOne">deleteOne()</option>
					</select>
				)}

				<div className="flex-1" />

				<Button
					variant="ghost"
					size="sm"
					onClick={() => setHistoryOpen(true)}
					className="h-9 px-2 text-muted-foreground hover:text-foreground"
					title={t("query.history")}
				>
					<History size={16} className="mr-1" />
					{t("query.history")}
				</Button>

				<span className="text-xs text-muted-foreground hidden sm:inline-block">
					{t("query.runHint")}
				</span>

				{!sessionId && (
					<span className="flex items-center gap-1.5 text-xs text-warning bg-warning/10 px-2 py-1 rounded-full border border-warning/20">
						<AlertCircle size={12} /> {t("query.noConnection")}
					</span>
				)}
			</div>

			<div className="flex-1 min-h-50 border-b border-border relative">
				{isMongo ? (
					<MongoEditor
						value={query}
						onChange={setQuery}
						onExecute={() => handleExecute()}
						readOnly={loading}
					/>
				) : (
					<SQLEditor
						value={query}
						onChange={setQuery}
						onExecute={() => handleExecute()}
						onExecuteSelection={(selection) => handleExecute(selection)}
						dialect={dialect as "postgres" | "mysql"}
						readOnly={loading}
					/>
				)}
			</div>

			{/* Results / Error */}
			<div className="flex-1 flex flex-col min-h-0 bg-background overflow-hidden relative">
				{error ? (
					<div className="p-4 m-4 rounded-md bg-error/10 border border-error/20 text-error flex items-start gap-3">
						<AlertCircle className="mt-0.5 shrink-0" size={18} />
						<pre className="text-sm font-mono whitespace-pre-wrap break-all">
							{error}
						</pre>
					</div>
				) : result ? (
					isMongo ? (
						<div className="flex-1 overflow-hidden p-2 flex flex-col h-full">
							<DocumentResults
								result={result}
								sessionId={sessionId || undefined}
								database={connectionDatabase || 'admin'}
								collection={getCollectionFromQuery(query)}
								environment={environment}
								readOnly={readOnly}
								connectionName={connectionName}
								connectionDatabase={connectionDatabase}
								onRowsDeleted={runCurrentQuery}
								onEditDocument={handleEditDocument}
							/>
						</div>
					) : (
						<div className="flex-1 overflow-hidden p-2 flex flex-col h-full">
							{/* DataGrid fills container */}
							<DataGrid 
                                result={result}
                                sessionId={sessionId || undefined}
                                connectionName={connectionName}
                                connectionDatabase={connectionDatabase}
                                environment={environment}
                                readOnly={readOnly}
                             />
						</div>
					)
				) : (
					<div className="flex items-center justify-center h-full text-muted-foreground text-sm">
						{t("query.noResults")}
					</div>
				)}
			</div>

			{/* History Modal */}
			<QueryHistory
				isOpen={historyOpen}
				onClose={() => setHistoryOpen(false)}
				onSelectQuery={setQuery}
				sessionId={sessionId || undefined}
			/>

			<ProductionConfirmDialog
				open={confirmOpen}
				onOpenChange={(open) => {
					setConfirmOpen(open);
					if (!open) {
						setPendingQuery(null);
					}
				}}
				title={t("environment.confirmTitle")}
				confirmationLabel={
					(connectionDatabase || connectionName || "PROD").trim() || "PROD"
				}
				confirmLabel={t("common.confirm")}
				onConfirm={handleConfirm}
			/>

			<DangerConfirmDialog
				open={dangerConfirmOpen}
				onOpenChange={(open) => {
					setDangerConfirmOpen(open);
					if (!open) {
						setPendingQuery(null);
						setDangerConfirmInfo(undefined);
						setDangerConfirmLabel(undefined);
					}
				}}
				title={t("environment.dangerousQueryTitle")}
				description={t("environment.dangerousQuery")}
				warningInfo={dangerConfirmInfo}
				confirmationLabel={dangerConfirmLabel}
				confirmLabel={t("common.confirm")}
				onConfirm={handleDangerConfirm}
			/>

            <DocumentEditorModal
                isOpen={docModalOpen}
                onClose={() => setDocModalOpen(false)}
                mode={docModalMode}
                initialData={docModalData}
                sessionId={sessionId || ''}
                database={connectionDatabase || 'admin'} // Default to admin or context?
                collection={getCollectionFromQuery(query)}
                originalId={docOriginalId}
                onSuccess={() => {
                    handleExecute(); // Refresh data
                }}
                readOnly={readOnly}
            />
		</div>
	);
}

