// SPDX-License-Identifier: Apache-2.0

import {
  AlertCircle,
  Calendar,
  ChevronLeft,
  ChevronRight,
  Database,
  Eye,
  FunctionSquare,
  HardDrive,
  Hash,
  List,
  Loader2,
  PlayCircle,
  Plus,
  Search,
  Shield,
  ShieldAlert,
  Table,
  TerminalSquare,
  X,
  Zap,
} from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { LicenseGate } from '@/components/License/LicenseGate';
import { ERDiagram } from '@/components/Schema/ERDiagram';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { getTerminology } from '@/lib/driverCapabilities';
import { emitTableChange, onTableChange } from '@/lib/tableEvents';
import { cn } from '@/lib/utils';
import { DRIVER_ICONS, DRIVER_LABELS, type Driver, getDriverMetadata } from '../../lib/drivers';
import {
  type Collection,
  type DatabaseEvent,
  type Environment,
  executeQuery,
  listCollections,
  listEvents,
  listRoutines,
  listTriggers,
  type Namespace,
  type RelationFilter,
  type Routine,
  type Trigger,
} from '../../lib/tauri';
import { CreateTableModal } from '../Table/CreateTableModal';
import { StatCard } from './StatCard';

function formatBytes(bytes: number): string {
  if (!bytes || bytes < 1024) return `${bytes || 0} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export type DatabaseBrowserTab = 'overview' | 'tables' | 'routines' | 'triggers' | 'schema';

interface DatabaseBrowserProps {
  sessionId: string;
  namespace: Namespace;
  driver: Driver;
  environment?: Environment;
  readOnly?: boolean;
  connectionName?: string;
  connectionId?: string;
  onTableSelect: (namespace: Namespace, tableName: string, relationFilter?: RelationFilter) => void;
  schemaRefreshTrigger?: number;
  onSchemaChange?: () => void;
  onOpenQueryTab?: (namespace: Namespace) => void;
  onOpenFulltextSearch?: () => void;
  onClose: () => void;
  initialTab?: DatabaseBrowserTab;
  onActiveTabChange?: (tab: DatabaseBrowserTab) => void;
}

interface DatabaseStats {
  sizeBytes?: number;
  sizeFormatted?: string;
  tableCount?: number;
  indexCount?: number;
  documentCount?: number;
}

export function DatabaseBrowser({
  sessionId,
  namespace,
  driver,
  environment = 'development',
  readOnly = false,
  connectionName,
  connectionId,
  onTableSelect,
  schemaRefreshTrigger,
  onSchemaChange,
  onOpenQueryTab,
  onOpenFulltextSearch,
  onClose,
  initialTab,
  onActiveTabChange,
}: DatabaseBrowserProps) {
  const { t } = useTranslation();
  const terminology = getTerminology(driver);
  // Stabilize namespace reference to prevent infinite re-render loops
  // when parent creates a new object with same values each render
  const nsDatabase = namespace.database;
  const nsSchema = namespace.schema;
  const stableNamespace = useMemo<Namespace>(
    () => ({ database: nsDatabase, schema: nsSchema }),
    [nsDatabase, nsSchema]
  );
  const [activeTab, setActiveTab] = useState<DatabaseBrowserTab>(initialTab ?? 'overview');
  const [stats, setStats] = useState<DatabaseStats>({});
  const [collections, setCollections] = useState<Collection[]>([]);
  const [routines, setRoutines] = useState<Routine[]>([]);
  const [routinesLoading, setRoutinesLoading] = useState(false);
  const [triggers, setTriggers] = useState<Trigger[]>([]);
  const [triggersLoading, setTriggersLoading] = useState(false);
  const [dbEvents, setDbEvents] = useState<DatabaseEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [createTableOpen, setCreateTableOpen] = useState(false);

  // Search & Pagination
  const [search, setSearch] = useState('');
  const [page, setPage] = useState(1);
  const [totalCount, setTotalCount] = useState(0);
  const pageSize = 20;

  const driverMeta = getDriverMetadata(driver);

  const handleTabChange = useCallback(
    (tab: DatabaseBrowserTab) => {
      setActiveTab(tab);
      onActiveTabChange?.(tab);
    },
    [onActiveTabChange]
  );

  const loadRoutines = useCallback(async () => {
    setRoutinesLoading(true);
    try {
      const result = await listRoutines(sessionId, stableNamespace);
      if (result.success && result.data) {
        setRoutines(result.data.routines);
      }
    } catch (err) {
      console.error('Failed to load routines:', err);
    } finally {
      setRoutinesLoading(false);
    }
  }, [sessionId, stableNamespace]);

  const loadTriggers = useCallback(async () => {
    setTriggersLoading(true);
    try {
      const result = await listTriggers(sessionId, stableNamespace);
      if (result.success && result.data) {
        setTriggers(result.data.triggers);
      }
      if (driver === 'mysql') {
        const eventsResult = await listEvents(sessionId, stableNamespace);
        if (eventsResult.success && eventsResult.data) {
          setDbEvents(eventsResult.data.events);
        }
      }
    } catch (err) {
      console.error('Failed to load triggers:', err);
    } finally {
      setTriggersLoading(false);
    }
  }, [sessionId, stableNamespace, driver]);

  const loadData = useCallback(async () => {
    if (activeTab === 'schema') {
      setLoading(false);
      return;
    }
    if (activeTab === 'routines') {
      setLoading(false);
      loadRoutines();
      return;
    }
    if (activeTab === 'triggers') {
      setLoading(false);
      loadTriggers();
      return;
    }
    setLoading(true);
    setError(null);

    try {
      // Determine fetch options based on tab
      const isOverview = activeTab === 'overview';
      const fetchPage = isOverview ? 1 : page;
      const fetchLimit = isOverview ? 10 : pageSize;
      const fetchSearch = isOverview ? undefined : search || undefined;

      // Load collections
      const collectionsResult = await listCollections(
        sessionId,
        stableNamespace,
        fetchSearch,
        fetchPage,
        fetchLimit
      );

      if (collectionsResult.success && collectionsResult.data) {
        setCollections(collectionsResult.data.collections);
        setTotalCount(collectionsResult.data.total_count);
      }

      const newStats: DatabaseStats = {
        tableCount: collectionsResult.data?.total_count || 0,
      };

      if (driverMeta.supportsSQL && isOverview) {
        const schemaOrDb = stableNamespace.schema || stableNamespace.database;
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
              newStats.indexCount =
                typeof rawValue === 'number' ? rawValue : parseInt(String(rawValue), 10) || 0;
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
  }, [
    activeTab,
    loadRoutines,
    loadTriggers,
    page,
    search,
    sessionId,
    stableNamespace,
    driverMeta.supportsSQL,
    driverMeta.queries,
  ]);

  useEffect(() => {
    if (!driverMeta.supportsSQL && activeTab === 'schema') {
      handleTabChange('overview');
    }
  }, [activeTab, driverMeta.supportsSQL, handleTabChange]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  useEffect(() => {
    return onTableChange(event => {
      if (event.type !== 'create' && event.type !== 'drop') {
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

  useEffect(() => {
    if (schemaRefreshTrigger === undefined) return;
    loadData();
  }, [schemaRefreshTrigger, loadData]);

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
            <img src={iconSrc} alt={DRIVER_LABELS[driver]} className="w-4 h-4 object-contain" />
          </div>
          <div>
            <h2 className="font-semibold text-foreground flex items-center gap-2">
              {displayName}
              {connectionName && (
                <span className="text-xs text-muted-foreground font-normal">
                  ({connectionName})
                </span>
              )}
            </h2>
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <span>{DRIVER_LABELS[driver]}</span>
              <span>•</span>
              <span
                className={cn(
                  'flex items-center gap-1',
                  environment === 'production' && 'text-destructive'
                )}
              >
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
          {onOpenFulltextSearch && (
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8"
              onClick={onOpenFulltextSearch}
              title={t('fulltextSearch.title')}
            >
              <Search size={16} />
            </Button>
          )}
          {driverMeta.supportsSQL && (
            <Button
              variant="outline"
              size="sm"
              className="h-8 gap-1.5 text-xs"
              onClick={() => onOpenQueryTab?.(namespace)}
            >
              <TerminalSquare size={14} />
              {t('databaseBrowser.openEditor')}
            </Button>
          )}
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
          type="button"
          className={cn(
            'px-3 py-1.5 text-sm font-medium rounded-md transition-colors',
            activeTab === 'overview'
              ? 'bg-accent text-accent-foreground'
              : 'text-muted-foreground hover:text-foreground hover:bg-muted'
          )}
          onClick={() => handleTabChange('overview')}
        >
          <span className="flex items-center gap-2">
            <Database size={14} />
            {t('databaseBrowser.overview')}
          </span>
        </button>
        <button
          type="button"
          className={cn(
            'px-3 py-1.5 text-sm font-medium rounded-md transition-colors',
            activeTab === 'tables'
              ? 'bg-accent text-accent-foreground'
              : 'text-muted-foreground hover:text-foreground hover:bg-muted'
          )}
          onClick={() => handleTabChange('tables')}
        >
          <span className="flex items-center gap-2">
            <Table size={14} />
            {t(terminology.tablePluralLabel)} ({totalCount})
          </span>
        </button>
        {driverMeta.supportsSQL && (
          <button
            type="button"
            className={cn(
              'px-3 py-1.5 text-sm font-medium rounded-md transition-colors',
              activeTab === 'routines'
                ? 'bg-accent text-accent-foreground'
                : 'text-muted-foreground hover:text-foreground hover:bg-muted'
            )}
            onClick={() => handleTabChange('routines')}
          >
            <span className="flex items-center gap-2">
              <FunctionSquare size={14} />
              {t('databaseBrowser.routines')} ({routines.length})
            </span>
          </button>
        )}
        {driverMeta.supportsSQL && (
          <button
            type="button"
            className={cn(
              'px-3 py-1.5 text-sm font-medium rounded-md transition-colors',
              activeTab === 'triggers'
                ? 'bg-accent text-accent-foreground'
                : 'text-muted-foreground hover:text-foreground hover:bg-muted'
            )}
            onClick={() => handleTabChange('triggers')}
          >
            <span className="flex items-center gap-2">
              <Zap size={14} />
              {t('databaseBrowser.triggers')} ({triggers.length})
            </span>
          </button>
        )}
        {driverMeta.supportsSQL && (
          <button
            type="button"
            className={cn(
              'px-3 py-1.5 text-sm font-medium rounded-md transition-colors',
              activeTab === 'schema'
                ? 'bg-accent text-accent-foreground'
                : 'text-muted-foreground hover:text-foreground hover:bg-muted'
            )}
            onClick={() => handleTabChange('schema')}
          >
            <span className="flex items-center gap-2">
              <List size={14} />
              {t('databaseBrowser.schema')}
            </span>
          </button>
        )}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-4">
        {activeTab === 'overview' ? (
          loading ? (
            <div className="flex items-center justify-center h-full gap-2 text-muted-foreground">
              <Loader2 size={20} className="animate-spin" />
              <span>{t('common.loading')}</span>
            </div>
          ) : error ? (
            <div className="flex items-center gap-3 p-4 rounded-md bg-error/10 border border-error/20 text-error">
              <AlertCircle size={18} />
              <pre className="text-sm font-mono whitespace-pre-wrap">{error}</pre>
            </div>
          ) : (
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
                  label={t(terminology.tablePluralLabel)}
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
                  {t(terminology.tablePluralLabel)}
                </h3>
                {collections.length === 0 ? (
                  <div className="text-sm text-muted-foreground italic p-4 text-center border border-dashed border-border rounded-md">
                    {t('databaseBrowser.noTables')}
                  </div>
                ) : (
                  <div className="border border-border rounded-md divide-y divide-border">
                    {collections.slice(0, 10).map(col => (
                      <button
                        type="button"
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
                        type="button"
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
          )
        ) : activeTab === 'tables' ? (
          /* Tables Tab */
          <div className="flex flex-col h-full gap-4">
            <div className="flex items-center gap-2">
              <div className="relative flex-1">
                <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
                <Input
                  placeholder={t('databaseBrowser.searchTables')}
                  value={search}
                  onChange={e => {
                    setSearch(e.target.value);
                    setPage(1);
                  }}
                  className="pl-9"
                />
              </div>
            </div>

            <div className="border border-border rounded-md divide-y divide-border flex-1 overflow-auto relative min-h-50">
              {loading && (
                <div className="absolute inset-0 z-10 bg-background/50 flex items-center justify-center backdrop-blur-[1px]">
                  <Loader2 size={24} className="animate-spin text-primary" />
                </div>
              )}

              {!loading && error ? (
                <div className="flex items-center gap-3 p-4 m-4 rounded-md bg-error/10 border border-error/20 text-error">
                  <AlertCircle size={18} />
                  <pre className="text-sm font-mono whitespace-pre-wrap">{error}</pre>
                </div>
              ) : collections.length === 0 && !loading ? (
                <div className="text-sm text-muted-foreground italic p-8 text-center">
                  {search ? t('databaseBrowser.noResults') : t('databaseBrowser.noTables')}
                </div>
              ) : (
                collections.map(col => (
                  <button
                    type="button"
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

            {/* Pagination */}
            <div className="flex items-center justify-between border-t border-border pt-4">
              <div className="text-sm text-muted-foreground">
                {t('common.pagination', {
                  start: totalCount === 0 ? 0 : (page - 1) * pageSize + 1,
                  end: Math.min(page * pageSize, totalCount),
                  total: totalCount,
                })}
              </div>
              <div className="flex items-center gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setPage(p => Math.max(1, p - 1))}
                  disabled={page === 1 || loading}
                >
                  <ChevronLeft size={16} />
                </Button>
                <div className="text-sm font-medium w-8 text-center">{page}</div>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setPage(p => p + 1)}
                  disabled={page * pageSize >= totalCount || loading}
                >
                  <ChevronRight size={16} />
                </Button>
              </div>
            </div>
          </div>
        ) : activeTab === 'routines' ? (
          /* Routines Tab */
          <div className="flex flex-col h-full gap-4">
            <div className="border border-border rounded-md divide-y divide-border flex-1 overflow-auto relative min-h-50">
              {routinesLoading && (
                <div className="absolute inset-0 z-10 bg-background/50 flex items-center justify-center backdrop-blur-[1px]">
                  <Loader2 size={24} className="animate-spin text-primary" />
                </div>
              )}

              {routines.length === 0 && !routinesLoading ? (
                <div className="text-sm text-muted-foreground italic p-8 text-center">
                  {t('databaseBrowser.noRoutines')}
                </div>
              ) : (
                <>
                  {/* Functions */}
                  {routines.filter(r => r.routine_type === 'Function').length > 0 && (
                    <div className="p-3 bg-muted/30">
                      <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-2">
                        <FunctionSquare size={12} />
                        {t('dbtree.functions')} (
                        {routines.filter(r => r.routine_type === 'Function').length})
                      </h4>
                    </div>
                  )}
                  {routines
                    .filter(r => r.routine_type === 'Function')
                    .map(routine => (
                      <div
                        key={`fn-${routine.name}-${routine.arguments}`}
                        className="flex items-center justify-between w-full px-4 py-3 hover:bg-muted/50 transition-colors text-left"
                      >
                        <div className="flex items-center gap-3">
                          <FunctionSquare size={16} className="text-muted-foreground" />
                          <div>
                            <span className="font-mono text-sm">{routine.name}</span>
                            <span className="text-xs text-muted-foreground ml-1">
                              ({routine.arguments})
                            </span>
                            {routine.return_type && (
                              <span className="text-xs text-muted-foreground ml-1">
                                &rarr; {routine.return_type}
                              </span>
                            )}
                          </div>
                        </div>
                        {routine.language && (
                          <span className="text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded">
                            {routine.language}
                          </span>
                        )}
                      </div>
                    ))}

                  {/* Procedures */}
                  {routines.filter(r => r.routine_type === 'Procedure').length > 0 && (
                    <div className="p-3 bg-muted/30">
                      <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-2">
                        <PlayCircle size={12} />
                        {t('dbtree.procedures')} (
                        {routines.filter(r => r.routine_type === 'Procedure').length})
                      </h4>
                    </div>
                  )}
                  {routines
                    .filter(r => r.routine_type === 'Procedure')
                    .map(routine => (
                      <div
                        key={`proc-${routine.name}-${routine.arguments}`}
                        className="flex items-center justify-between w-full px-4 py-3 hover:bg-muted/50 transition-colors text-left"
                      >
                        <div className="flex items-center gap-3">
                          <PlayCircle size={16} className="text-muted-foreground" />
                          <div>
                            <span className="font-mono text-sm">{routine.name}</span>
                            <span className="text-xs text-muted-foreground ml-1">
                              ({routine.arguments})
                            </span>
                          </div>
                        </div>
                        {routine.language && (
                          <span className="text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded">
                            {routine.language}
                          </span>
                        )}
                      </div>
                    ))}
                </>
              )}
            </div>
          </div>
        ) : activeTab === 'triggers' ? (
          /* Triggers Tab */
          <div className="flex flex-col h-full gap-4">
            <div className="border border-border rounded-md divide-y divide-border flex-1 overflow-auto relative min-h-50">
              {triggersLoading && (
                <div className="absolute inset-0 z-10 bg-background/50 flex items-center justify-center backdrop-blur-[1px]">
                  <Loader2 size={24} className="animate-spin text-primary" />
                </div>
              )}

              {triggers.length === 0 && dbEvents.length === 0 && !triggersLoading ? (
                <div className="text-sm text-muted-foreground italic p-8 text-center">
                  {t('databaseBrowser.noTriggers')}
                </div>
              ) : (
                <>
                  {/* Triggers */}
                  {triggers.length > 0 && (
                    <>
                      <div className="p-3 bg-muted/30">
                        <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-2">
                          <Zap size={12} />
                          {t('databaseBrowser.triggers')} ({triggers.length})
                        </h4>
                      </div>
                      {triggers.map(trigger => (
                        <div
                          key={trigger.name}
                          className="flex items-center justify-between w-full px-4 py-3 hover:bg-muted/50 transition-colors text-left"
                        >
                          <div className="flex items-center gap-3">
                            <Zap
                              size={16}
                              className={cn(
                                'text-muted-foreground',
                                !trigger.enabled && 'opacity-40'
                              )}
                            />
                            <div>
                              <span className="font-mono text-sm">{trigger.name}</span>
                              <span className="text-xs text-muted-foreground ml-2">
                                {trigger.timing} {trigger.events.join(' | ')} ON{' '}
                                {trigger.table_name}
                              </span>
                            </div>
                          </div>
                          <div className="flex items-center gap-2">
                            {trigger.function_name && (
                              <span className="text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded">
                                {trigger.function_name}
                              </span>
                            )}
                            {!trigger.enabled && (
                              <span className="text-xs text-orange-500 bg-orange-500/10 px-2 py-0.5 rounded">
                                disabled
                              </span>
                            )}
                          </div>
                        </div>
                      ))}
                    </>
                  )}

                  {/* MySQL Events */}
                  {dbEvents.length > 0 && (
                    <>
                      <div className="p-3 bg-muted/30">
                        <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-2">
                          <Calendar size={12} />
                          {t('databaseBrowser.events')} ({dbEvents.length})
                        </h4>
                      </div>
                      {dbEvents.map(evt => (
                        <div
                          key={evt.name}
                          className="flex items-center justify-between w-full px-4 py-3 hover:bg-muted/50 transition-colors text-left"
                        >
                          <div className="flex items-center gap-3">
                            <Calendar size={16} className="text-muted-foreground" />
                            <div>
                              <span className="font-mono text-sm">{evt.name}</span>
                              <span className="text-xs text-muted-foreground ml-2">
                                {evt.event_type}
                                {evt.interval_value && evt.interval_field && (
                                  <>
                                    {' '}
                                    every {evt.interval_value} {evt.interval_field}
                                  </>
                                )}
                              </span>
                            </div>
                          </div>
                          <span
                            className={cn(
                              'text-xs px-2 py-0.5 rounded',
                              evt.status === 'Enabled'
                                ? 'text-emerald-600 bg-emerald-500/10'
                                : 'text-orange-500 bg-orange-500/10'
                            )}
                          >
                            {evt.status}
                          </span>
                        </div>
                      ))}
                    </>
                  )}
                </>
              )}
            </div>
          </div>
        ) : (
          <div className="h-full">
            <LicenseGate feature="er_diagram">
              <ERDiagram
                sessionId={sessionId}
                namespace={namespace}
                connectionId={connectionId}
                schemaRefreshTrigger={schemaRefreshTrigger}
                onTableSelect={onTableSelect}
              />
            </LicenseGate>
          </div>
        )}
      </div>
      <CreateTableModal
        isOpen={createTableOpen}
        onClose={() => setCreateTableOpen(false)}
        sessionId={sessionId}
        namespace={namespace}
        driver={driver}
        onTableCreated={tableName => {
          loadData();
          if (activeTab === 'tables') {
            loadData();
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
