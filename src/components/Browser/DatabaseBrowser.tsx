import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  Namespace, 
  Collection,
  listCollections,
  executeQuery,
  Environment
} from '../../lib/tauri';
import { cn } from '@/lib/utils';
import { 
  Database,
  Table,
  Eye,
  Loader2, 
  AlertCircle,
  X,
  HardDrive,
  List,
  Hash,
  ChevronRight,
  Shield,
  ShieldAlert,
  Plus
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { getDriverMetadata, Driver, DRIVER_LABELS, DRIVER_ICONS } from '../../lib/drivers';
import { CreateTableModal } from '../Table/CreateTableModal';
import { emitTableChange, onTableChange } from '@/lib/tableEvents';

interface DatabaseBrowserProps {
  sessionId: string;
  namespace: Namespace;
  driver: Driver;
  environment?: Environment;
  readOnly?: boolean;
  connectionName?: string;
  onTableSelect: (namespace: Namespace, tableName: string) => void;
  onSchemaChange?: () => void;
  onClose: () => void;
}

interface DatabaseStats {
  sizeBytes?: number;
  sizeFormatted?: string;
  tableCount?: number;
  indexCount?: number;
  documentCount?: number;
}

type Tab = 'overview' | 'tables';

export function DatabaseBrowser({
  sessionId,
  namespace,
  driver,
  environment = 'development',
  readOnly = false,
  connectionName,
  onTableSelect,
  onSchemaChange,
  onClose,
}: DatabaseBrowserProps) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<Tab>('overview');
  const [stats, setStats] = useState<DatabaseStats>({});
  const [collections, setCollections] = useState<Collection[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [createTableOpen, setCreateTableOpen] = useState(false);

  const driverMeta = getDriverMetadata(driver);

  const loadData = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      // Load collections
      const collectionsResult = await listCollections(sessionId, namespace);
      if (collectionsResult.success && collectionsResult.collections) {
        setCollections(collectionsResult.collections);
      }

      const newStats: DatabaseStats = {
        tableCount: collectionsResult.collections?.length || 0,
      };

      if (driverMeta.supportsSQL) {
        const schemaOrDb = namespace.schema || namespace.database;
        const queries = driverMeta.queries;

        // Database/schema size
        if (queries.databaseSizeQuery) {
          try {
            const sizeQuery = queries.databaseSizeQuery(schemaOrDb);
            const sizeResult = await executeQuery(sessionId, sizeQuery);
            if (sizeResult.success && sizeResult.result?.rows[0]) {
              const rawValue = sizeResult.result.rows[0].values[0];
              if (typeof rawValue === 'string') {
                newStats.sizeFormatted = rawValue;
              } else if (typeof rawValue === 'number') {
                newStats.sizeBytes = rawValue;
                newStats.sizeFormatted = formatBytes(rawValue);
              } else if (rawValue !== null) {
                const bytes = parseFloat(String(rawValue)) || 0;
                if (bytes > 0) {
                  newStats.sizeBytes = bytes;
                  newStats.sizeFormatted = formatBytes(bytes);
                }
              }
            }
          } catch (err) {
            console.error('[DatabaseBrowser] Size query error:', err);
          }
        }

        // Index count
        if (queries.indexCountQuery) {
          try {
            const indexQuery = queries.indexCountQuery(schemaOrDb);
            const indexResult = await executeQuery(sessionId, indexQuery);
            if (indexResult.success && indexResult.result?.rows[0]) {
              const rawValue = indexResult.result.rows[0].values[0];
              newStats.indexCount = typeof rawValue === 'number' 
                ? rawValue 
                : parseInt(String(rawValue), 10) || 0;
            }
          } catch (err) {
            console.error('[DatabaseBrowser] Index query error:', err);
          }
        }
      }

      setStats(newStats);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load database info');
    } finally {
      setLoading(false);
    }
  }, [sessionId, namespace, driverMeta]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  useEffect(() => {
    return onTableChange((event) => {
      if (
        event.type !== 'create' &&
        event.type !== 'drop'
      ) {
        return;
      }
      if (
        event.namespace.database === namespace.database &&
        (event.namespace.schema || '') === (namespace.schema || '')
      ) {
        loadData();
      }
    });
  }, [loadData, namespace.database, namespace.schema]);

  function formatBytes(bytes: number): string {
    if (!bytes || bytes < 1024) return `${bytes || 0} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
    return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
  }

  const displayName = namespace.schema 
    ? `${namespace.database}.${namespace.schema}` 
    : namespace.database;

  const iconSrc = `/databases/${DRIVER_ICONS[driver]}`;


  return (
    <div className="flex flex-col h-full bg-background rounded-lg border border-border shadow-sm overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border bg-muted/20">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-md bg-accent/10 text-accent">
            <img src={iconSrc} alt={DRIVER_LABELS[driver]} className="w-4 h-4"/>
          </div>
          <div>
            <h2 className="font-semibold text-foreground flex items-center gap-2">
              {displayName}
              {connectionName && (
                <span className="text-xs text-muted-foreground font-normal">({connectionName})</span>
              )}
            </h2>
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <span>{DRIVER_LABELS[driver]}</span>
              <span>•</span>
              <span className={cn(
                "flex items-center gap-1",
                environment === 'production' && "text-destructive"
              )}>
                {environment === 'production' ? <ShieldAlert size={10} /> : <Shield size={10} />}
                {t(`environment.${environment}`)}
              </span>
              {readOnly && (
                <>
                  <span>•</span>
                  <span className="text-warning">{t('environment.readOnly')}</span>
                </>
              )}
            </div>
          </div>
        </div>
        <div className="flex items-center gap-1">
          {driverMeta.supportsSQL && !readOnly && (
            <Button 
              variant="ghost" 
              size="icon" 
              onClick={() => setCreateTableOpen(true)} 
              className="h-8 w-8"
              title={t('createTable.title')}
            >
              <Plus size={16} />
            </Button>
          )}
          <Button variant="ghost" size="icon" onClick={onClose} className="h-8 w-8">
            <X size={16} />
          </Button>
        </div>
      </div>

      {/* Tabs */}
      <div className="flex items-center gap-1 px-4 py-2 border-b border-border bg-muted/10">
        <button
          className={cn(
            "px-3 py-1.5 text-sm font-medium rounded-md transition-colors",
            activeTab === 'overview' 
              ? "bg-accent text-accent-foreground" 
              : "text-muted-foreground hover:text-foreground hover:bg-muted"
          )}
          onClick={() => setActiveTab('overview')}
        >
          <span className="flex items-center gap-2">
            <Database size={14} />
            {t('databaseBrowser.overview')}
          </span>
        </button>
        <button
          className={cn(
            "px-3 py-1.5 text-sm font-medium rounded-md transition-colors",
            activeTab === 'tables' 
              ? "bg-accent text-accent-foreground" 
              : "text-muted-foreground hover:text-foreground hover:bg-muted"
          )}
          onClick={() => setActiveTab('tables')}
        >
          <span className="flex items-center gap-2">
            <Table size={14} />
            {t('databaseBrowser.tables')} ({collections.length})
          </span>
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-4">
        {loading ? (
          <div className="flex items-center justify-center h-full gap-2 text-muted-foreground">
            <Loader2 size={20} className="animate-spin" />
            <span>{t('common.loading')}</span>
          </div>
        ) : error ? (
          <div className="flex items-center gap-3 p-4 rounded-md bg-error/10 border border-error/20 text-error">
            <AlertCircle size={18} />
            <pre className="text-sm font-mono whitespace-pre-wrap">{error}</pre>
          </div>
        ) : activeTab === 'overview' ? (
          <div className="space-y-6">
            {/* Stats Grid */}
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              {stats.sizeFormatted && (
                <StatCard 
                  icon={<HardDrive size={16} />}
                  label={t('databaseBrowser.size')}
                  value={stats.sizeFormatted}
                />
              )}
              <StatCard 
                icon={<List size={16} />}
                label={t('databaseBrowser.tableCount')}
                value={stats.tableCount?.toString() || '0'}
              />
              {stats.indexCount !== undefined && (
                <StatCard 
                  icon={<Hash size={16} />}
                  label={t('databaseBrowser.indexCount')}
                  value={stats.indexCount.toString()}
                />
              )}
            </div>

            {/* Quick Tables List */}
            <div className="space-y-2">
              <h3 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
                {t('databaseBrowser.tables')}
              </h3>
              {collections.length === 0 ? (
                <div className="text-sm text-muted-foreground italic p-4 text-center border border-dashed border-border rounded-md">
                  {t('databaseBrowser.noTables')}
                </div>
              ) : (
                <div className="border border-border rounded-md divide-y divide-border">
                  {collections.slice(0, 10).map(col => (
                    <button
                      key={col.name}
                      className="flex items-center justify-between w-full px-3 py-2 hover:bg-muted/50 transition-colors text-left"
                      onClick={() => onTableSelect(namespace, col.name)}
                    >
                      <div className="flex items-center gap-2">
                        {col.collection_type === 'View' ? (
                          <Eye size={14} className="text-muted-foreground" />
                        ) : (
                          <Table size={14} className="text-muted-foreground" />
                        )}
                        <span className="font-mono text-sm">{col.name}</span>
                        {col.collection_type === 'View' && (
                          <span className="text-xs text-muted-foreground">(view)</span>
                        )}
                      </div>
                      <ChevronRight size={14} className="text-muted-foreground" />
                    </button>
                  ))}
                  {collections.length > 10 && (
                    <button
                      className="w-full px-3 py-2 text-sm text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
                      onClick={() => setActiveTab('tables')}
                    >
                      {t('databaseBrowser.viewAll', { count: collections.length })}
                    </button>
                  )}
                </div>
              )}
            </div>
          </div>
        ) : (
          /* Tables Tab */
          <div className="border border-border rounded-md divide-y divide-border">
            {collections.length === 0 ? (
              <div className="text-sm text-muted-foreground italic p-8 text-center">
                {t('databaseBrowser.noTables')}
              </div>
            ) : (
              collections.map(col => (
                <button
                  key={col.name}
                  className="flex items-center justify-between w-full px-4 py-3 hover:bg-muted/50 transition-colors text-left"
                  onClick={() => onTableSelect(namespace, col.name)}
                >
                  <div className="flex items-center gap-3">
                    {col.collection_type === 'View' ? (
                      <Eye size={16} className="text-muted-foreground" />
                    ) : (
                      <Table size={16} className="text-muted-foreground" />
                    )}
                    <div>
                      <span className="font-mono text-sm">{col.name}</span>
                      {col.collection_type === 'View' && (
                        <span className="ml-2 text-xs text-muted-foreground">(view)</span>
                      )}
                    </div>
                  </div>
                  <ChevronRight size={16} className="text-muted-foreground" />
                </button>
              ))
            )}
          </div>
        )}
      </div>
      <CreateTableModal
        isOpen={createTableOpen}
        onClose={() => setCreateTableOpen(false)}
        sessionId={sessionId}
        namespace={namespace}
        driver={driver}
        onTableCreated={(tableName) => {
          loadData();
          if (activeTab === 'tables') {
            loadData(); // Re-fetch to update tables list
          }
          onSchemaChange?.();
          if (tableName) {
            emitTableChange({ type: 'create', namespace, tableName });
          }
        }}
      />
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
    <div className="flex items-center gap-3 p-4 rounded-md border border-border bg-muted/20">
      <div className="text-accent">{icon}</div>
      <div>
        <div className="text-xs text-muted-foreground">{label}</div>
        <div className="text-lg font-semibold">{value}</div>
      </div>
    </div>
  );
}
