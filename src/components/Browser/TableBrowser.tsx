import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Namespace,
  TableSchema,
  QueryResult,
  queryTable,
  TableQueryOptions,
  executeQuery,
  Environment,
  DriverCapabilities,
  RelationFilter,
  SearchFilter,
  peekForeignKey,
  Value,
  generateMigrationSql,
  applySandboxChanges,
  SandboxChangeDto,
} from '../../lib/tauri';
import { useSchemaCache } from '../../hooks/useSchemaCache';
import { ResultsViewer } from '../Results/ResultsViewer';
import { DocumentEditorModal } from '../Editor/DocumentEditorModal';
import { cn } from '@/lib/utils';
import {
  Table,
  Columns3,
  Database,
  Key,
  Hash,
  Loader2,
  AlertCircle,
  X,
  Plus,
  Info,
  HardDrive,
  List,
  Clock,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { RowModal } from './RowModal';
import { toast } from 'sonner';
import { Driver, getDriverMetadata } from '../../lib/drivers';
import { buildQualifiedTableName } from '@/lib/column-types';
import { isDocumentDatabase } from '../../lib/driverCapabilities';
import { onTableChange } from '@/lib/tableEvents';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import { SandboxToggle, ChangesPanel, MigrationPreview } from '@/components/Sandbox';
import {
  isSandboxActive,
  getChangesForTable,
  createInsertChange,
  createUpdateChange,
  createDeleteChange,
  clearSandboxChanges,
  getSandboxSession,
  getSandboxPreferences,
  subscribeSandbox,
  subscribeSandboxPreferences,
  saveSandboxBackup,
  getSandboxBackup,
  clearSandboxBackup,
  importChanges,
  activateSandbox,
  deactivateSandbox,
} from '@/lib/sandboxStore';
import { SandboxChange, MigrationScript } from '@/lib/sandboxTypes';
import { UI_EVENT_REFRESH_TABLE } from '@/lib/uiEvents';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';

function formatTableName(namespace: Namespace, tableName: string): string {
  return namespace.schema ? `${namespace.schema}.${tableName}` : tableName;
}

function buildStreamingExportQuery(driver: Driver, namespace: Namespace, tableName: string): string {
  const metadata = getDriverMetadata(driver);

  if (metadata.isDocumentBased) {
    return JSON.stringify({
      database: namespace.database,
      collection: tableName,
      query: {},
    });
  }

  const qualified = buildQualifiedTableName(namespace, tableName, driver);
  return `SELECT * FROM ${qualified};`;
}

function schemasCompatible(a: TableSchema, b: TableSchema): boolean {
  const mapA = new Map(a.columns.map(col => [col.name, col]));
  const mapB = new Map(b.columns.map(col => [col.name, col]));

  if (mapA.size !== mapB.size) return false;
  for (const [name, colA] of mapA) {
    const colB = mapB.get(name);
    if (!colB) return false;
    if (colA.data_type.toLowerCase() !== colB.data_type.toLowerCase()) return false;
    if (colA.nullable !== colB.nullable) return false;
  }

  const pkA = new Set(a.primary_key ?? []);
  const pkB = new Set(b.primary_key ?? []);
  if (pkA.size !== pkB.size) return false;
  for (const col of pkA) {
    if (!pkB.has(col)) return false;
  }

  return true;
}

export type TableBrowserTab = 'structure' | 'data' | 'info';

interface TableBrowserProps {
  sessionId: string;
  namespace: Namespace;
  tableName: string;
  driver?: Driver;
  driverCapabilities?: DriverCapabilities | null;
  environment?: Environment;
  readOnly?: boolean;
  connectionName?: string;
  connectionDatabase?: string;
  connectionId?: string;
  onClose: () => void;
  onOpenRelatedTable?: (namespace: Namespace, tableName: string) => void;
  relationFilter?: RelationFilter;
  searchFilter?: SearchFilter;
  initialTab?: TableBrowserTab;
  onActiveTabChange?: (tab: TableBrowserTab) => void;
}

export function TableBrowser({
  sessionId,
  namespace,
  tableName,
  driver = Driver.Postgres,
  driverCapabilities = null,
  environment = 'development',
  readOnly = false,
  connectionName,
  connectionDatabase,
  connectionId,
  onClose,
  onOpenRelatedTable,
  relationFilter,
  searchFilter,
  initialTab,
  onActiveTabChange,
}: TableBrowserProps) {
  const { t } = useTranslation();
  const viewTrackedRef = useRef(false);
  const [activeTab, setActiveTab] = useState<TableBrowserTab>(initialTab ?? 'data');
  const [schema, setSchema] = useState<TableSchema | null>(null);
  const [data, setData] = useState<QueryResult | null>(null);
  const hasDataRef = useRef(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Pagination state
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(50);
  const [totalRows, setTotalRows] = useState(0);

  // Search state
  const [searchTerm, setSearchTerm] = useState(searchFilter?.value ?? '');
  const [debouncedSearchTerm, setDebouncedSearchTerm] = useState(searchFilter?.value ?? '');

  // Debounce search term
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedSearchTerm(searchTerm);
    }, 500);
    return () => clearTimeout(timer);
  }, [searchTerm]);

  useEffect(() => {
    hasDataRef.current = Boolean(data);
  }, [data]);

  const handleServerSearchChange = useCallback((term: string) => {
    setSearchTerm(prev => (prev !== term ? term : prev));
  }, []);

  useEffect(() => {
    if (searchFilter?.value) {
      setSearchTerm(prev => (prev !== searchFilter.value ? searchFilter.value : prev));
      setDebouncedSearchTerm(prev => (prev !== searchFilter.value ? searchFilter.value : prev));
    }
  }, [searchFilter]);

  // Reset page when search changes
  useEffect(() => {
    setPage(1);
  }, [debouncedSearchTerm]);

  // Modal state
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [modalMode, setModalMode] = useState<'insert' | 'update'>('insert');
  const [selectedRow, setSelectedRow] = useState<Record<string, Value> | undefined>(undefined);
  const mutationsSupported = driverCapabilities?.mutations ?? true;

  // Document editor state
  const isDocument = isDocumentDatabase(driver);

  const streamingExportQuery = useMemo(() => {
    if (!namespace || !tableName || relationFilter) return undefined;
    return buildStreamingExportQuery(driver, namespace, tableName);
  }, [driver, namespace, tableName, relationFilter]);

  const [docEditorOpen, setDocEditorOpen] = useState(false);
  const [docEditorMode, setDocEditorMode] = useState<'insert' | 'edit'>('insert');
  const [docEditorData, setDocEditorData] = useState<string>('{}');
  const [docOriginalId, setDocOriginalId] = useState<Value | undefined>(undefined);

  // Sandbox state
  const [sandboxActive, setSandboxActive] = useState(() => isSandboxActive(sessionId));
  const [sandboxChanges, setSandboxChanges] = useState<SandboxChange[]>([]);
  const [changesPanelOpen, setChangesPanelOpen] = useState(false);
  const [migrationPreviewOpen, setMigrationPreviewOpen] = useState(false);
  const [migrationScript, setMigrationScript] = useState<MigrationScript | null>(null);
  const [migrationLoading, setMigrationLoading] = useState(false);
  const [migrationError, setMigrationError] = useState<string | null>(null);

  const handleTabChange = useCallback(
    (tab: TableBrowserTab) => {
      setActiveTab(tab);
      onActiveTabChange?.(tab);
    },
    [onActiveTabChange]
  );
  const [sandboxPrefs, setSandboxPrefs] = useState(() => getSandboxPreferences());
  const [restoreBackupOpen, setRestoreBackupOpen] = useState(false);
  const [pendingBackup, setPendingBackup] = useState<{
    changes: SandboxChange[];
    savedAt: number;
  } | null>(null);

  // Schema cache
  const schemaCache = useSchemaCache(sessionId);

  useEffect(() => {
    const unsubscribe = subscribeSandboxPreferences(prefs => {
      setSandboxPrefs(prefs);
    });
    return unsubscribe;
  }, []);

  const validateSandboxChanges = useCallback(
    async (changes: SandboxChange[]) => {
      const warnings: string[] = [];
      const errors: string[] = [];

      const tableMap = new Map<
        string,
        { namespace: Namespace; tableName: string; schema?: TableSchema; changes: SandboxChange[] }
      >();

      for (const change of changes) {
        const key = `${change.namespace.database}:${change.namespace.schema ?? ''}:${change.tableName}`;
        const entry = tableMap.get(key);
        if (entry) {
          entry.changes.push(change);
          if (!entry.schema && change.schema) {
            entry.schema = change.schema;
          }
        } else {
          tableMap.set(key, {
            namespace: change.namespace,
            tableName: change.tableName,
            schema: change.schema,
            changes: [change],
          });
        }
      }

      for (const entry of tableMap.values()) {
        const currentSchema = await schemaCache.getTableSchema(entry.namespace, entry.tableName);
        const displayName = formatTableName(entry.namespace, entry.tableName);

        if (!currentSchema) {
          warnings.push(t('sandbox.validation.schemaMissing', { table: displayName }));
          continue;
        }

        if (entry.schema && !schemasCompatible(entry.schema, currentSchema)) {
          errors.push(t('sandbox.validation.schemaMismatch', { table: displayName }));
        }

        const hasWriteChanges = entry.changes.some(
          change => change.type === 'update' || change.type === 'delete'
        );

        if (
          hasWriteChanges &&
          (!currentSchema.primary_key || currentSchema.primary_key.length === 0)
        ) {
          warnings.push(t('sandbox.validation.noPrimaryKey', { table: displayName }));
        }

        for (const change of entry.changes) {
          if ((change.type === 'update' || change.type === 'delete') && !change.primaryKey) {
            errors.push(t('sandbox.validation.missingPrimaryKey', { table: displayName }));
            break;
          }
        }
      }

      return { warnings, errors };
    },
    [schemaCache, t]
  );

  const loadData = useCallback(async () => {
    if (!hasDataRef.current) setLoading(true);
    setError(null);

    try {
      const startTime = performance.now();

      // For relation filters, use previewTable (limited view)
      // For normal table view, use queryTable with pagination
      if (relationFilter) {
        const [cachedSchema, dataResult] = await Promise.all([
          schemaCache.getTableSchema(namespace, tableName),
          peekForeignKey(
            sessionId,
            namespace,
            relationFilter.foreignKey,
            relationFilter.value,
            100
          ),
        ]);
        const endTime = performance.now();
        const totalTime = endTime - startTime;

        if (cachedSchema) {
          setSchema(cachedSchema);
        } else {
          setError('Failed to load table schema');
        }

        if (dataResult.success && dataResult.result) {
          const hydratedResult: QueryResult = {
            ...dataResult.result,
            columns:
              dataResult.result.columns.length === 0 && cachedSchema?.columns?.length
                ? cachedSchema.columns.map(c => ({
                    name: c.name,
                    data_type: c.data_type,
                    nullable: c.nullable,
                  }))
                : dataResult.result.columns,
          };

          setData({
            ...hydratedResult,
            total_time_ms: totalTime,
          } as QueryResult & { total_time_ms: number });
          setTotalRows(dataResult.result.rows.length);
        } else if (dataResult.error) {
          setError(dataResult.error);
        }
      } else {
        const options: TableQueryOptions = {
          page: !relationFilter ? page : 1,
          page_size: pageSize,
          search: debouncedSearchTerm,
        };

        const [cachedSchema, dataResult] = await Promise.all([
          schemaCache.getTableSchema(namespace, tableName),
          queryTable(sessionId, namespace, tableName, options),
        ]);
        const endTime = performance.now();
        const totalTime = endTime - startTime;

        if (cachedSchema) {
          setSchema(cachedSchema);
        } else {
          setError('Failed to load table schema');
        }

        if (dataResult.success && dataResult.result) {
          const paginatedResult = dataResult.result;
          const hydratedResult: QueryResult = {
            ...paginatedResult.result,
            columns:
              paginatedResult.result.columns.length === 0 && cachedSchema?.columns?.length
                ? cachedSchema.columns.map(c => ({
                    name: c.name,
                    data_type: c.data_type,
                    nullable: c.nullable,
                  }))
                : paginatedResult.result.columns,
          };

          setData({
            ...hydratedResult,
            total_time_ms: totalTime,
          } as QueryResult & { total_time_ms: number });
          setTotalRows(paginatedResult.total_rows);

          if (!viewTrackedRef.current) {
            viewTrackedRef.current = true;
            AnalyticsService.capture('table_view_loaded', {
              driver,
              resource_type: driver === Driver.Mongodb ? 'collection' : 'table',
            });
          }
        } else if (dataResult.error) {
          setError(dataResult.error);
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load table data');
    } finally {
      setLoading(false);
    }
  }, [
    relationFilter,
    sessionId,
    namespace,
    tableName,
    schemaCache,
    driver,
    page,
    pageSize,
    debouncedSearchTerm,
  ]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  useEffect(() => {
    const handler = () => {
      schemaCache.forceRefresh();
      loadData();
    };
    window.addEventListener(UI_EVENT_REFRESH_TABLE, handler);
    return () => window.removeEventListener(UI_EVENT_REFRESH_TABLE, handler);
  }, [loadData, schemaCache]);

  useEffect(() => {
    return onTableChange(event => {
      if (
        event.tableName === tableName &&
        event.namespace.database === namespace.database &&
        (event.namespace.schema || '') === (namespace.schema || '')
      ) {
        loadData();
      }
    });
  }, [loadData, namespace.database, namespace.schema, tableName]);

  // Sandbox subscription
  useEffect(() => {
    const loadSandboxState = () => {
      setSandboxActive(isSandboxActive(sessionId));
      const changes = getChangesForTable(sessionId, namespace, tableName);
      setSandboxChanges(changes);
      if (sandboxPrefs.autoCollapsePanel && changes.length === 0) {
        setChangesPanelOpen(false);
      }
      if (connectionId) {
        saveSandboxBackup(connectionId, sessionId);
      }
    };

    loadSandboxState();

    const unsubscribe = subscribeSandbox(changedSessionId => {
      if (changedSessionId === sessionId) {
        loadSandboxState();
      }
    });

    return unsubscribe;
  }, [sessionId, namespace, tableName, sandboxPrefs.autoCollapsePanel, connectionId]);

  useEffect(() => {
    return () => {
      window.setTimeout(() => {
        const session = getSandboxSession(sessionId);
        if (session.isActive && session.changes.length === 0) {
          deactivateSandbox(sessionId);
        }
      }, 0);
    };
  }, [sessionId]);

  useEffect(() => {
    if (!connectionId) return;
    const backup = getSandboxBackup(connectionId);
    if (!backup || backup.changes.length === 0) return;
    const current = getSandboxSession(sessionId);
    if (current.changes.length > 0) return;
    setPendingBackup({ changes: backup.changes, savedAt: backup.savedAt });
    setRestoreBackupOpen(true);
  }, [connectionId, sessionId]);

  // Sandbox handlers
  const handleSandboxInsert = useCallback(
    (newValues: Record<string, Value>) => {
      createInsertChange(sessionId, namespace, tableName, newValues, schema ?? undefined);
    },
    [sessionId, namespace, tableName, schema]
  );

  const handleSandboxUpdate = useCallback(
    (
      primaryKey: Record<string, Value>,
      oldValues: Record<string, Value>,
      newValues: Record<string, Value>
    ) => {
      createUpdateChange(
        sessionId,
        namespace,
        tableName,
        { columns: primaryKey },
        oldValues,
        newValues,
        schema ?? undefined
      );
    },
    [sessionId, namespace, tableName, schema]
  );

  const handleSandboxDelete = useCallback(
    (primaryKey: Record<string, Value>, oldValues: Record<string, Value>) => {
      createDeleteChange(
        sessionId,
        namespace,
        tableName,
        { columns: primaryKey },
        oldValues,
        schema ?? undefined
      );
    },
    [sessionId, namespace, tableName, schema]
  );

  const handleGenerateSQL = useCallback(async () => {
    setMigrationLoading(true);
    setMigrationError(null);

    try {
      const session = getSandboxSession(sessionId);
      const validation = await validateSandboxChanges(session.changes);

      if (validation.errors.length > 0) {
        setMigrationError(validation.errors.join('\n'));
        setMigrationLoading(false);
        return;
      }

      const changes: SandboxChangeDto[] = session.changes.map(c => ({
        change_type: c.type,
        namespace: c.namespace,
        table_name: c.tableName,
        primary_key: c.primaryKey,
        old_values: c.oldValues,
        new_values: c.newValues,
      }));

      const result = await generateMigrationSql(sessionId, changes);
      if (result.success && result.script) {
        const mergedWarnings = [...validation.warnings, ...(result.script.warnings ?? [])];
        setMigrationScript({
          ...result.script,
          warnings: mergedWarnings,
        });
        setMigrationPreviewOpen(true);
      } else {
        setMigrationError(result.error || 'Failed to generate SQL');
      }
    } catch (err) {
      setMigrationError(err instanceof Error ? err.message : 'Failed to generate SQL');
    } finally {
      setMigrationLoading(false);
    }
  }, [sessionId, validateSandboxChanges]);

  const handleApplySandbox = useCallback(async () => {
    const session = getSandboxSession(sessionId);
    const validation = await validateSandboxChanges(session.changes);
    if (validation.errors.length > 0) {
      const error = validation.errors.join('\n');
      return {
        success: false,
        applied_count: 0,
        error,
        failed_changes: [],
      };
    }

    const changes: SandboxChangeDto[] = session.changes.map(c => ({
      change_type: c.type,
      namespace: c.namespace,
      table_name: c.tableName,
      primary_key: c.primaryKey,
      old_values: c.oldValues,
      new_values: c.newValues,
    }));

    const result = await applySandboxChanges(sessionId, changes, true);

    if (result.success) {
      clearSandboxChanges(sessionId);
      deactivateSandbox(sessionId, true);
      loadData();
      if (sandboxPrefs.autoCollapsePanel) {
        setChangesPanelOpen(false);
      }
    }

    return result;
  }, [sessionId, loadData, sandboxPrefs.autoCollapsePanel, validateSandboxChanges]);

  const displayName = namespace.schema ? `${namespace.schema}.${tableName}` : tableName;

  return (
    <div className="flex flex-col h-full bg-background rounded-lg border border-border shadow-sm overflow-hidden">
      <div className="flex items-center justify-between px-4 py-3 border-b border-border bg-muted/20">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-md bg-(--q-accent-soft) text-(--q-accent)">
            <Table size={18} />
          </div>
          <div>
            <h2 className="font-semibold text-foreground">{displayName}</h2>
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <Database size={12} />
              <span>{namespace.database}</span>
              {typeof schema?.row_count_estimate === 'number' && (
                <>
                  <span>•</span>
                  <span>
                    ~{schema.row_count_estimate.toLocaleString()} {t('table.rows')}
                  </span>
                </>
              )}
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <SandboxToggle
            sessionId={sessionId}
            environment={environment}
            onToggle={active => {
              setSandboxActive(active);
              if (active) {
                setChangesPanelOpen(true);
              }
            }}
          />

          {sandboxActive && sandboxChanges.length > 0 && (
            <Button
              variant="outline"
              size="sm"
              className="h-8"
              onClick={() => setChangesPanelOpen(true)}
            >
              {t('sandbox.changes.count', { count: sandboxChanges.length })}
            </Button>
          )}

          <Button
            variant="outline"
            size="sm"
            className="h-8 gap-1.5"
            disabled={readOnly || !mutationsSupported}
            title={
              readOnly
                ? t('environment.blocked')
                : !mutationsSupported
                  ? t('grid.mutationsNotSupported')
                  : undefined
            }
            onClick={() => {
              if (readOnly) {
                toast.error(t('environment.blocked'));
                return;
              }
              if (!mutationsSupported) {
                toast.error(t('grid.mutationsNotSupported'));
                return;
              }
              if (isDocument) {
                // NoSQL: open document editor
                setDocEditorMode('insert');
                setDocEditorData('{}');
                setDocOriginalId(undefined);
                setDocEditorOpen(true);
              } else {
                // SQL: open row modal
                setModalMode('insert');
                setSelectedRow(undefined);
                setIsModalOpen(true);
              }
            }}
          >
            <Plus size={14} />
            {t('common.insert')}
          </Button>
          <Button variant="ghost" size="icon" onClick={onClose} className="h-8 w-8">
            <X size={16} />
          </Button>
        </div>
      </div>

      {/* Tabs */}
      <div className="flex items-center gap-1 px-4 py-2 border-b border-border bg-muted/10">
        <button
          className={cn(
            'px-3 py-1.5 text-sm font-medium rounded-md transition-colors',
            activeTab === 'data'
              ? 'bg-accent text-accent-foreground'
              : 'text-muted-foreground hover:text-foreground hover:bg-muted'
          )}
          onClick={() => handleTabChange('data')}
        >
          <span className="flex items-center gap-2">
            <Columns3 size={14} />
            {t('table.data')}
          </span>
        </button>
        {!isDocument && (
          <button
            className={cn(
              'px-3 py-1.5 text-sm font-medium rounded-md transition-colors',
              activeTab === 'structure'
                ? 'bg-accent text-accent-foreground'
                : 'text-muted-foreground hover:text-foreground hover:bg-muted'
            )}
            onClick={() => handleTabChange('structure')}
          >
            <span className="flex items-center gap-2">
              <Key size={14} />
              {t('table.structure')}
            </span>
          </button>
        )}
        <button
          className={cn(
            'px-3 py-1.5 text-sm font-medium rounded-md transition-colors',
            activeTab === 'info'
              ? 'bg-accent text-accent-foreground'
              : 'text-muted-foreground hover:text-foreground hover:bg-muted'
          )}
          onClick={() => handleTabChange('info')}
        >
          <span className="flex items-center gap-2">
            <Info size={14} />
            {t('table.info')}
          </span>
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-4">
        {loading ? (
          <div className="flex items-center justify-center h-full gap-2 text-muted-foreground">
            <Loader2 size={20} className="animate-spin" />
            <span>{t('table.loading')}</span>
          </div>
        ) : error ? (
          <div className="flex items-center gap-3 p-4 rounded-md bg-error/10 border border-error/20 text-error">
            <AlertCircle size={18} />
            <pre className="text-sm font-mono whitespace-pre-wrap">{error}</pre>
          </div>
        ) : activeTab === 'data' ? (
          <ResultsViewer
            result={data}
            driver={driver}
            sessionId={sessionId}
            environment={environment}
            readOnly={readOnly}
            connectionName={connectionName}
            connectionDatabase={connectionDatabase}
            onRowsDeleted={loadData}
            namespace={namespace}
            tableName={tableName}
            tableSchema={schema}
            primaryKey={schema?.primary_key ?? undefined}
            mutationsSupported={mutationsSupported}
            initialFilter={searchFilter?.value}
            onRowsUpdated={loadData}
            onOpenRelatedTable={onOpenRelatedTable}
            sandboxMode={sandboxActive}
            pendingChanges={sandboxChanges}
            sandboxDeleteDisplay={sandboxPrefs.deleteDisplay}
            onSandboxUpdate={handleSandboxUpdate}
            onSandboxDelete={handleSandboxDelete}
            exportQuery={streamingExportQuery}
            exportNamespace={namespace}
            serverSideTotalRows={!relationFilter ? totalRows : undefined}
            serverSidePage={!relationFilter ? page : undefined}
            serverSidePageSize={!relationFilter ? pageSize : undefined}
            onServerPageChange={!relationFilter ? setPage : undefined}
            onServerPageSizeChange={!relationFilter ? setPageSize : undefined}
            serverSearchTerm={!relationFilter ? searchTerm : undefined}
            onServerSearchChange={!relationFilter ? handleServerSearchChange : undefined}
            onRowClick={row => {
              if (readOnly) {
                toast.error(t('environment.blocked'));
                return;
              }
              if (!mutationsSupported) {
                toast.error(t('grid.mutationsNotSupported'));
                return;
              }
              setModalMode('update');
              setSelectedRow(row);
              setIsModalOpen(true);
            }}
            database={namespace.database}
            collection={tableName}
            onEditDocument={(doc, idValue) => {
              if (readOnly) {
                toast.error(t('environment.blocked'));
                return;
              }
              setDocEditorMode('edit');
              setDocEditorData(JSON.stringify(doc, null, 2));
              setDocOriginalId(idValue);
              setDocEditorOpen(true);
            }}
          />
        ) : activeTab === 'structure' ? (
          <StructureTable schema={schema} />
        ) : (
          <TableInfoPanel
            sessionId={sessionId}
            namespace={namespace}
            tableName={tableName}
            driver={driver}
            schema={schema}
          />
        )}
      </div>

      {schema && !isDocument && (
        <RowModal
          isOpen={isModalOpen}
          onClose={() => setIsModalOpen(false)}
          mode={modalMode}
          sessionId={sessionId}
          namespace={namespace}
          tableName={tableName}
          schema={schema}
          driver={driver}
          environment={environment}
          connectionName={connectionName}
          connectionDatabase={connectionDatabase}
          readOnly={readOnly}
          initialData={selectedRow}
          onSuccess={loadData}
          sandboxMode={sandboxActive}
          onSandboxInsert={handleSandboxInsert}
          onSandboxUpdate={handleSandboxUpdate}
        />
      )}

      {isDocument && (
        <DocumentEditorModal
          isOpen={docEditorOpen}
          onClose={() => setDocEditorOpen(false)}
          mode={docEditorMode}
          sessionId={sessionId}
          database={namespace.database}
          collection={tableName}
          initialData={docEditorData}
          originalId={docOriginalId}
          onSuccess={loadData}
          readOnly={readOnly}
          environment={environment}
          connectionName={connectionName}
          connectionDatabase={connectionDatabase}
        />
      )}

      {connectionId && (
        <Dialog
          open={restoreBackupOpen}
          onOpenChange={open => {
            setRestoreBackupOpen(open);
            if (!open) {
              setPendingBackup(null);
            }
          }}
        >
          <DialogContent className="max-w-md">
            <DialogHeader>
              <DialogTitle>{t('sandbox.restore.title')}</DialogTitle>
            </DialogHeader>
            <div className="text-sm text-muted-foreground space-y-2">
              <p>
                {t('sandbox.restore.message', {
                  count: pendingBackup?.changes.length ?? 0,
                })}
              </p>
              {pendingBackup?.savedAt && (
                <p>
                  {t('sandbox.restore.savedAt', {
                    date: new Date(pendingBackup.savedAt).toLocaleString(),
                  })}
                </p>
              )}
            </div>
            <DialogFooter className="gap-2">
              <Button
                variant="outline"
                onClick={() => {
                  clearSandboxBackup(connectionId);
                  setRestoreBackupOpen(false);
                }}
              >
                {t('sandbox.restore.discard')}
              </Button>
              <Button
                onClick={() => {
                  if (pendingBackup?.changes?.length) {
                    importChanges(sessionId, pendingBackup.changes);
                    activateSandbox(sessionId);
                    clearSandboxBackup(connectionId);
                  }
                  setRestoreBackupOpen(false);
                }}
              >
                {t('sandbox.restore.restore')}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      )}

      {/* Sandbox Changes Panel */}
      <ChangesPanel
        sessionId={sessionId}
        isOpen={changesPanelOpen}
        onClose={() => setChangesPanelOpen(false)}
        onGenerateSQL={handleGenerateSQL}
        environment={environment}
      />

      {/* Migration Preview Modal */}
      <MigrationPreview
        isOpen={migrationPreviewOpen}
        onClose={() => setMigrationPreviewOpen(false)}
        script={migrationScript}
        loading={migrationLoading}
        error={migrationError}
        environment={environment}
        dialect={driver}
        onApply={handleApplySandbox}
      />
    </div>
  );
}

interface StructureTableProps {
  schema: TableSchema | null;
}

function StructureTable({ schema }: StructureTableProps) {
  const { t } = useTranslation();

  if (!schema || schema.columns.length === 0) {
    return (
      <div className="flex items-center justify-center h-40 text-muted-foreground text-sm">
        {t('table.noSchema')}
      </div>
    );
  }

  return (
    <div className="border border-border rounded-md overflow-hidden">
      {/* Header */}
      <div className="flex items-center bg-muted/50 border-b border-border text-xs font-semibold text-muted-foreground uppercase tracking-wider">
        <div className="w-8 p-2 text-center">#</div>
        <div className="flex-1 p-2">{t('table.column')}</div>
        <div className="w-40 p-2">{t('table.type')}</div>
        <div className="w-24 p-2 text-center">{t('table.nullable')}</div>
        <div className="w-48 p-2">{t('table.default')}</div>
      </div>

      {/* Rows */}
      {schema.columns.map((col, idx) => (
        <div
          key={col.name}
          className="flex items-center border-b border-border last:border-b-0 hover:bg-muted/30 transition-colors text-sm"
        >
          <div className="w-8 p-2 text-center text-muted-foreground text-xs">{idx + 1}</div>
          <div className="flex-1 p-2 font-mono flex items-center gap-2">
            {col.is_primary_key && <Key size={12} className="text-warning shrink-0" />}
            <span className={cn(col.is_primary_key && 'font-semibold')}>{col.name}</span>
          </div>
          <div className="w-40 p-2 font-mono text-xs text-accent truncate" title={col.data_type}>{col.data_type}</div>
          <div className="w-24 p-2 text-center">
            {col.nullable ? (
              <span className="text-muted-foreground">NULL</span>
            ) : (
              <span className="text-foreground font-medium">NOT NULL</span>
            )}
          </div>
          <div className="w-48 p-2 font-mono text-xs text-muted-foreground truncate">
            {col.default_value || '—'}
          </div>
        </div>
      ))}

      {/* Primary Key Info */}
      {schema.primary_key && schema.primary_key.length > 0 && (
        <div className="flex items-center gap-2 p-3 bg-warning/10 border-t border-warning/20 text-sm">
          <Hash size={14} className="text-warning" />
          <span className="text-muted-foreground">{t('table.primaryKey')}:</span>
          <span className="font-mono font-medium">{schema.primary_key.join(', ')}</span>
        </div>
      )}
    </div>
  );
}

// ==================== Table Info Panel ====================

interface TableStats {
  sizeBytes?: number;
  sizeFormatted?: string;
  rowCount?: number;
  indexCount?: number;
  indexes?: Array<{
    name: string;
    columns: string;
    size?: string;
  }>;
  lastVacuum?: string;
  lastAnalyze?: string;
}

interface TableInfoPanelProps {
  sessionId: string;
  namespace: Namespace;
  tableName: string;
  driver: string;
  schema: TableSchema | null;
}

function TableInfoPanel({ sessionId, namespace, tableName, driver, schema }: TableInfoPanelProps) {
  const { t } = useTranslation();
  const [stats, setStats] = useState<TableStats>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const driverMeta = getDriverMetadata(driver);
  const loadStats = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      const schemaName = namespace.schema || 'public';
      const newStats: TableStats = {};

      if (driverMeta.supportsSQL) {
        // PostgreSQL stats query
        if (driver === Driver.Postgres) {
          //TODO : à passer en backend ?
          const sizeQuery = `
            SELECT pg_total_relation_size('"${schemaName}"."${tableName}"') as total_bytes,
                   pg_size_pretty(pg_total_relation_size('"${schemaName}"."${tableName}"')) as size_pretty
          `;
          const sizeResult = await executeQuery(sessionId, sizeQuery);
          if (sizeResult.success && sizeResult.result?.rows[0]) {
            const row = sizeResult.result.rows[0].values;
            newStats.sizeBytes = row[0] as number;
            newStats.sizeFormatted = row[1] as string;
          }

          // Row count (exact)
          //TODO : count is heavy , may by have a fallack for larges databases
          const countQuery = `SELECT COUNT(*) as cnt FROM "${schemaName}"."${tableName}"`;
          const countResult = await executeQuery(sessionId, countQuery);
          if (countResult.success && countResult.result?.rows[0]) {
            newStats.rowCount = countResult.result.rows[0].values[0] as number;
          }

          // Indexes
          const indexQuery = `
            SELECT indexname, indexdef
            FROM pg_indexes
            WHERE schemaname = '${schemaName}' AND tablename = '${tableName}'
            ORDER BY indexname
          `;
          const indexResult = await executeQuery(sessionId, indexQuery);
          if (indexResult.success && indexResult.result) {
            newStats.indexes = indexResult.result.rows.map(row => ({
              name: row.values[0] as string,
              columns: (row.values[1] as string).replace(/.*\((.*?)\).*/, '$1'),
            }));
            newStats.indexCount = newStats.indexes.length;
          }

          // Last vacuum/analyze
          const maintenanceQuery = `
            SELECT last_vacuum, last_analyze
            FROM pg_stat_user_tables
            WHERE schemaname = '${schemaName}' AND relname = '${tableName}'
          `;
          const maintenanceResult = await executeQuery(sessionId, maintenanceQuery);
          if (maintenanceResult.success && maintenanceResult.result?.rows[0]) {
            const row = maintenanceResult.result.rows[0].values;
            newStats.lastVacuum = (row[0] as string) || undefined;
            newStats.lastAnalyze = (row[1] as string) || undefined;
          }
        }
        // MySQL/MariaDB
        else if (driver === Driver.Mysql) {
          const statsQuery = `
            SELECT data_length + index_length as total_bytes, table_rows
            FROM information_schema.tables 
            WHERE table_schema = '${namespace.database}' AND table_name = '${tableName}'
          `;
          const statsResult = await executeQuery(sessionId, statsQuery);
          if (statsResult.success && statsResult.result?.rows[0]) {
            const row = statsResult.result.rows[0].values;
            newStats.sizeBytes = row[0] as number;
            newStats.sizeFormatted = formatBytes(row[0] as number);
            newStats.rowCount = row[1] as number;
          }

          // Indexes
          const indexQuery = `SHOW INDEX FROM \`${tableName}\``;
          const indexResult = await executeQuery(sessionId, indexQuery);
          if (indexResult.success && indexResult.result) {
            const indexMap = new Map<string, string[]>();
            for (const row of indexResult.result.rows) {
              const name = row.values[2] as string;
              const col = row.values[4] as string;
              if (!indexMap.has(name)) indexMap.set(name, []);
              indexMap.get(name)!.push(col);
            }
            newStats.indexes = Array.from(indexMap.entries()).map(([name, cols]) => ({
              name,
              columns: cols.join(', '),
            }));
            newStats.indexCount = newStats.indexes.length;
          }
        }
      }

      setStats(newStats);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load stats');
    } finally {
      setLoading(false);
    }
  }, [sessionId, namespace, tableName, driver, driverMeta]);

  useEffect(() => {
    loadStats();
  }, [loadStats]);

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
    return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-40 gap-2 text-muted-foreground">
        <Loader2 size={20} className="animate-spin" />
        <span>{t('common.loading')}</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center gap-3 p-4 rounded-md bg-error/10 border border-error/20 text-error">
        <AlertCircle size={18} />
        <span className="text-sm">{error}</span>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Overview Stats */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <StatCard
          icon={<HardDrive size={16} />}
          label={t('tableInfo.size')}
          value={stats.sizeFormatted || '—'}
        />
        <StatCard
          icon={<List size={16} />}
          label={t('tableInfo.rowCount')}
          value={stats.rowCount !== undefined ? stats.rowCount.toLocaleString() : '—'}
        />
        <StatCard
          icon={<Key size={16} />}
          label={t('tableInfo.columnCount')}
          value={schema?.columns.length?.toString() || '—'}
        />
        <StatCard
          icon={<Hash size={16} />}
          label={t('tableInfo.indexCount')}
          value={stats.indexCount?.toString() || '—'}
        />
      </div>

      {/* Indexes */}
      {stats.indexes && stats.indexes.length > 0 && (
        <div className="border border-border rounded-md overflow-hidden">
          <div className="px-3 py-2 bg-muted/50 border-b border-border text-xs font-semibold text-muted-foreground uppercase">
            {t('tableInfo.indexes')}
          </div>
          <div className="divide-y divide-border">
            {stats.indexes.map(idx => (
              <div key={idx.name} className="flex items-center justify-between px-3 py-2 text-sm">
                <span className="font-mono font-medium">{idx.name}</span>
                <span className="text-muted-foreground font-mono text-xs">{idx.columns}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Maintenance Info (PostgreSQL) */}
      {(stats.lastVacuum || stats.lastAnalyze) && (
        <div className="border border-border rounded-md overflow-hidden">
          <div className="px-3 py-2 bg-muted/50 border-b border-border text-xs font-semibold text-muted-foreground uppercase">
            {t('tableInfo.maintenance')}
          </div>
          <div className="px-3 py-2 space-y-1 text-sm">
            {stats.lastVacuum && (
              <div className="flex items-center gap-2">
                <Clock size={14} className="text-muted-foreground" />
                <span className="text-muted-foreground">{t('tableInfo.lastVacuum')}:</span>
                <span className="font-mono text-xs">{stats.lastVacuum}</span>
              </div>
            )}
            {stats.lastAnalyze && (
              <div className="flex items-center gap-2">
                <Clock size={14} className="text-muted-foreground" />
                <span className="text-muted-foreground">{t('tableInfo.lastAnalyze')}:</span>
                <span className="font-mono text-xs">{stats.lastAnalyze}</span>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

interface StatCardProps {
  icon: React.ReactNode;
  label: string;
  value: string;
}

function StatCard({ icon, label, value }: StatCardProps) {
  return (
    <div className="flex items-center gap-3 p-3 rounded-md border border-border bg-muted/20">
      <div className="text-muted-foreground">{icon}</div>
      <div>
        <div className="text-xs text-muted-foreground">{label}</div>
        <div className="text-sm font-semibold">{value}</div>
      </div>
    </div>
  );
}
