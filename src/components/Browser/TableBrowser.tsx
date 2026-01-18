import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  Namespace, 
  TableSchema, 
  QueryResult,
  previewTable,
  executeQuery,
  Environment 
} from '../../lib/tauri';
import { useSchemaCache } from '../../hooks/useSchemaCache';
import { DataGrid } from '../Grid/DataGrid';
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
  Clock
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Value } from '../../lib/tauri';
import { RowModal } from './RowModal'
import { toast } from 'sonner';
import { getDriverMetadata } from '../../lib/drivers';
import { onTableChange } from '@/lib/tableEvents';

interface TableBrowserProps {
  sessionId: string;
  namespace: Namespace;
  tableName: string;
  driver?: string;
  environment?: Environment;
  readOnly?: boolean;
  connectionName?: string;
  connectionDatabase?: string;
  onClose: () => void;
}

type Tab = 'structure' | 'data' | 'info';

export function TableBrowser({
  sessionId,
  namespace,
  tableName,
  driver = 'postgres',
  environment = 'development',
  readOnly = false,
  connectionName,
  connectionDatabase,
  onClose,
}: TableBrowserProps) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<Tab>('data');
  const [schema, setSchema] = useState<TableSchema | null>(null);
  const [data, setData] = useState<QueryResult | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Modal state
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [modalMode, setModalMode] = useState<'insert' | 'update'>('insert');
  const [selectedRow, setSelectedRow] = useState<Record<string, Value> | undefined>(undefined);

  // Schema cache
  const schemaCache = useSchemaCache(sessionId);

  const loadData = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      const startTime = performance.now();
      // Load schema from cache, data fresh
      const [cachedSchema, dataResult] = await Promise.all([
        schemaCache.getTableSchema(namespace, tableName),
        previewTable(sessionId, namespace, tableName, 100),
      ]);
      const endTime = performance.now();
      const totalTime = endTime - startTime;

      if (cachedSchema) {
        setSchema(cachedSchema);
      } else {
        setError('Failed to load table schema');
      }

      if (dataResult.success && dataResult.result) {
        setData({
          ...dataResult.result,
          total_time_ms: totalTime,
        } as QueryResult & { total_time_ms: number });
      } else if (dataResult.error && !error) {
        setError(dataResult.error);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load table data');
    } finally {
      setLoading(false);
    }
  }, [sessionId, namespace, tableName, schemaCache, error]);

  useEffect(() => {
    loadData();
  }, [loadData]);

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

  const displayName = namespace.schema ? `${namespace.schema}.${tableName}` : tableName;

  return (
    <div className="flex flex-col h-full bg-background rounded-lg border border-border shadow-sm overflow-hidden">
      {/* Header */}
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
              {schema?.row_count_estimate !== undefined && (
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
          <Button
            variant="outline"
            size="sm"
            className="h-8 gap-1.5"
            disabled={readOnly}
            title={readOnly ? t('environment.blocked') : undefined}
            onClick={() => {
              if (readOnly) {
                toast.error(t('environment.blocked'));
                return;
              }
              setModalMode('insert');
              setSelectedRow(undefined);
              setIsModalOpen(true);
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
          onClick={() => setActiveTab('data')}
        >
          <span className="flex items-center gap-2">
            <Columns3 size={14} />
            {t('table.data')}
          </span>
        </button>
        <button
          className={cn(
            'px-3 py-1.5 text-sm font-medium rounded-md transition-colors',
            activeTab === 'structure'
              ? 'bg-accent text-accent-foreground'
              : 'text-muted-foreground hover:text-foreground hover:bg-muted'
          )}
          onClick={() => setActiveTab('structure')}
        >
          <span className="flex items-center gap-2">
            <Key size={14} />
            {t('table.structure')}
          </span>
        </button>
        <button
          className={cn(
            'px-3 py-1.5 text-sm font-medium rounded-md transition-colors',
            activeTab === 'info'
              ? 'bg-accent text-accent-foreground'
              : 'text-muted-foreground hover:text-foreground hover:bg-muted'
          )}
          onClick={() => setActiveTab('info')}
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
          <DataGrid
            result={data}
            height={500}
            sessionId={sessionId}
            namespace={namespace}
            tableName={tableName}
            primaryKey={schema?.primary_key}
            environment={environment}
            readOnly={readOnly}
            connectionName={connectionName}
            connectionDatabase={connectionDatabase}
            onRowsDeleted={loadData}
            onRowClick={row => {
              if (readOnly) {
                toast.error(t('environment.blocked'));
                return;
              }
              setModalMode('update');
              setSelectedRow(row);
              setIsModalOpen(true);
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

      {schema && (
        <RowModal
          isOpen={isModalOpen}
          onClose={() => setIsModalOpen(false)}
          mode={modalMode}
          sessionId={sessionId}
          namespace={namespace}
          tableName={tableName}
          schema={schema}
          driver={driver}
          readOnly={readOnly}
          initialData={selectedRow}
          onSuccess={loadData}
        />
      )}
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
        <div className="w-32 p-2">{t('table.type')}</div>
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
          <div className="w-32 p-2 font-mono text-xs text-accent">{col.data_type}</div>
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
        if (driver === 'postgres') {
          // Table size
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
        else if (driver === 'mysql') {
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
