// SPDX-License-Identifier: Apache-2.0

import {
  Calendar,
  ChevronDown,
  ChevronRight,
  Database,
  Eye,
  FunctionSquare,
  Layers,
  Loader2,
  PlayCircle,
  Plus,
  Search,
  Table,
  Zap,
} from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { emitTableChange } from '@/lib/tableEvents';
import { cn } from '@/lib/utils';
import { useSchemaCache } from '../../hooks/useSchemaCache';
import { getTerminology } from '../../lib/driverCapabilities';
import { type Driver, getDriverMetadata } from '../../lib/drivers';
import {
  type Collection,
  type DatabaseEvent,
  listCollections,
  listEvents,
  listRoutines,
  listTriggers,
  type Namespace,
  type RelationFilter,
  type Routine,
  type SavedConnection,
  type Trigger,
} from '../../lib/tauri';
import { CreateTableModal } from '../Table/CreateTableModal';
import { CreateDatabaseModal } from './CreateDatabaseModal';
import { DatabaseContextMenu } from './DatabaseContextMenu';
import { DeleteDatabaseModal } from './DeleteDatabaseModal';
import { TableContextMenu } from './TableContextMenu';

function getNsKey(ns: Namespace): string {
  return `${ns.database}:${ns.schema || ''}`;
}

interface DBTreeProps {
  connectionId: string;
  driver: string;
  connection?: SavedConnection;
  onTableSelect?: (
    namespace: Namespace,
    tableName: string,
    relationFilter?: RelationFilter
  ) => void;
  onDatabaseSelect?: (namespace: Namespace) => void;
  onCompareTable?: (collection: Collection) => void;
  onAiGenerateForTable?: (collection: Collection) => void;
  refreshTrigger?: number;
  activeNamespace?: Namespace | null;
}

export function DBTree({
  connectionId,
  driver,
  connection,
  onTableSelect,
  onDatabaseSelect,
  onCompareTable,
  onAiGenerateForTable,
  refreshTrigger,
  activeNamespace,
}: DBTreeProps) {
  const { t } = useTranslation();
  const [namespaces, setNamespaces] = useState<Namespace[]>([]);
  const [expandedNs, setExpandedNs] = useState<string | null>(null);
  const [expandedNamespace, setExpandedNamespace] = useState<Namespace | null>(null);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [collectionsTotal, setCollectionsTotal] = useState(0);
  const [collectionsPage, setCollectionsPage] = useState(1);
  const [collectionsLoading, setCollectionsLoading] = useState(false);
  const [routines, setRoutines] = useState<Routine[]>([]);
  const [routinesLoading, setRoutinesLoading] = useState(false);
  const [triggers, setTriggers] = useState<Trigger[]>([]);
  const [triggersLoading, setTriggersLoading] = useState(false);
  const [dbEvents, setDbEvents] = useState<DatabaseEvent[]>([]);
  const [expandedSections, setExpandedSections] = useState<Set<string>>(new Set(['tables']));
  const schemaCache = useSchemaCache(connectionId, connection?.id);
  const [createModalOpen, setCreateModalOpen] = useState(false);
  const [createTableOpen, setCreateTableOpen] = useState(false);
  const [createTableNamespace, setCreateTableNamespace] = useState<Namespace | null>(null);
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [deleteTargetNamespace, setDeleteTargetNamespace] = useState<Namespace | null>(null);
  const [search, setSearch] = useState('');
  const [searchValue, setSearchValue] = useState('');
  const [collapsedActiveNsKey, setCollapsedActiveNsKey] = useState<string | null>(null);
  const collectionsPageSize = 50;

  const driverMeta = getDriverMetadata(driver);
  const terminology = getTerminology(driver);

  const sessionId = connectionId;
  const { getNamespaces, invalidateNamespaces } = schemaCache;

  // Debounce search
  useEffect(() => {
    const timer = setTimeout(() => {
      setSearch(searchValue);
    }, 300);
    return () => clearTimeout(timer);
  }, [searchValue]);

  const loadNamespaces = useCallback(async () => {
    try {
      const ns = await getNamespaces();
      setNamespaces(ns);
      return ns;
    } catch (err: unknown) {
      if (err instanceof Error) {
        console.error('Failed to load namespaces:', err);
        toast.error(`Failed to load databases: ${err.message}`);
      } else {
        console.error('Failed to load namespaces:', err);
        toast.error(`Failed to load databases: ${err}`);
      }
    }
    return [];
  }, [getNamespaces]);

  const refreshCollections = useCallback(
    async (ns: Namespace, page = 1, append = false) => {
      setCollectionsLoading(true);
      try {
        const cols = await listCollections(connectionId, ns, search, page, collectionsPageSize);
        if (!cols.success || !cols.data) return;

        const data = cols.data;
        setCollectionsTotal(data.total_count);
        setCollectionsPage(page);

        if (!append || (page === 1 && !append)) {
          setCollections(data.collections);
        } else {
          setCollections(prev => [...prev, ...data.collections]);
        }
      } catch (err) {
        console.error('Failed to refresh collections:', err);
      } finally {
        setCollectionsLoading(false);
      }
    },
    [connectionId, search]
  );

  const refreshRoutines = useCallback(
    async (ns: Namespace) => {
      setRoutinesLoading(true);
      try {
        const result = await listRoutines(connectionId, ns, search);
        if (result.success && result.data) {
          setRoutines(result.data.routines);
        }
      } catch (err) {
        console.error('Failed to refresh routines:', err);
      } finally {
        setRoutinesLoading(false);
      }
    },
    [connectionId, search]
  );

  const refreshTriggers = useCallback(
    async (ns: Namespace) => {
      setTriggersLoading(true);
      try {
        const result = await listTriggers(connectionId, ns, search);
        if (result.success && result.data) {
          setTriggers(result.data.triggers);
        }
        if (driver === 'mysql') {
          const eventsResult = await listEvents(connectionId, ns, search);
          if (eventsResult.success && eventsResult.data) {
            setDbEvents(eventsResult.data.events);
          }
        }
      } catch (err) {
        console.error('Failed to refresh triggers:', err);
      } finally {
        setTriggersLoading(false);
      }
    },
    [connectionId, search, driver]
  );

  const toggleSection = useCallback((section: string) => {
    setExpandedSections(prev => {
      const next = new Set(prev);
      if (next.has(section)) {
        next.delete(section);
      } else {
        next.add(section);
      }
      return next;
    });
  }, []);

  // Sync expanded state with activeNamespace
  useEffect(() => {
    if (activeNamespace) {
      const key = getNsKey(activeNamespace);
      if (collapsedActiveNsKey === key) {
        return;
      }
      if (collapsedActiveNsKey && collapsedActiveNsKey !== key) {
        setCollapsedActiveNsKey(null);
      }
      if (expandedNs !== key) {
        setExpandedNs(key);
        setExpandedNamespace(activeNamespace);
        refreshCollections(activeNamespace, 1, false);
      }
    }
  }, [activeNamespace, collapsedActiveNsKey, expandedNs, refreshCollections]);

  const canLoadMore = collections.length > 0 && collections.length < collectionsTotal;

  // Reload when search changes
  useEffect(() => {
    if (expandedNamespace) {
      refreshCollections(expandedNamespace, 1, false);
      refreshRoutines(expandedNamespace);
      refreshTriggers(expandedNamespace);
    }
  }, [expandedNamespace, refreshCollections, refreshRoutines, refreshTriggers]);

  const handleLoadMore = useCallback(async () => {
    if (!expandedNamespace || collectionsLoading) return;
    const nextPage = collectionsPage + 1;
    await refreshCollections(expandedNamespace, nextPage, true);
  }, [expandedNamespace, collectionsLoading, collectionsPage, refreshCollections]);

  const refreshExpandedNamespace = useCallback(async () => {
    if (!expandedNamespace) return;
    await refreshCollections(expandedNamespace, 1, false);
  }, [expandedNamespace, refreshCollections]);

  useEffect(() => {
    loadNamespaces();
  }, [loadNamespaces]);

  useEffect(() => {
    if (refreshTrigger === undefined) return;
    const refresh = async () => {
      invalidateNamespaces();
      const updated = await loadNamespaces();
      if (expandedNs && !updated.some(ns => getNsKey(ns) === expandedNs)) {
        setExpandedNs(null);
        setExpandedNamespace(null);
        setCollections([]);
        setCollectionsTotal(0);
        return;
      }
      await refreshExpandedNamespace();
    };
    refresh();
  }, [refreshTrigger, invalidateNamespaces, loadNamespaces, refreshExpandedNamespace, expandedNs]);

  async function handleExpandNamespace(ns: Namespace) {
    const key = `${ns.database}:${ns.schema || ''}`;

    if (expandedNs === key) {
      setExpandedNs(null);
      setExpandedNamespace(null);
      setCollections([]);
      setCollectionsTotal(0);
      setRoutines([]);
      setTriggers([]);
      setDbEvents([]);
      setSearch('');
      setSearchValue('');
      if (activeNamespace && getNsKey(activeNamespace) === key) {
        setCollapsedActiveNsKey(key);
      }
      return;
    }

    setExpandedNs(key);
    setExpandedNamespace(ns);
    setCollapsedActiveNsKey(null);
    setSearch('');
    setSearchValue('');
    await Promise.all([refreshCollections(ns, 1, false), refreshRoutines(ns), refreshTriggers(ns)]);
  }

  async function openNamespace(ns: Namespace) {
    const key = getNsKey(ns);
    if (expandedNs !== key) {
      setExpandedNs(key);
      setExpandedNamespace(ns);
      setCollapsedActiveNsKey(null);
      await Promise.all([
        refreshCollections(ns, 1, false),
        refreshRoutines(ns),
        refreshTriggers(ns),
      ]);
    }
    onDatabaseSelect?.(ns);
  }

  function handleTableClick(col: Collection) {
    onTableSelect?.(col.namespace, col.name);
  }

  if (schemaCache.loading && namespaces.length === 0) {
    return (
      <div className="flex items-center gap-2 p-2 text-sm text-muted-foreground animate-pulse">
        <Loader2 size={14} className="animate-spin" /> {t('common.loading')}
      </div>
    );
  }

  return (
    <div className="flex flex-col text-sm">
      <div className="flex items-center justify-between px-2 py-1.5 mb-1.5">
        <span className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/80">
          {t(driverMeta.treeRootLabel)}
        </span>
        {driverMeta.createAction !== 'none' && (
          <Button
            variant="ghost"
            size="icon"
            className="h-5 w-5 ml-2"
            onClick={() => setCreateModalOpen(true)}
            disabled={connection?.read_only}
            title={
              connection?.read_only
                ? t('environment.blocked')
                : t(
                    driverMeta.createAction === 'schema'
                      ? 'database.newSchema'
                      : 'database.newDatabase'
                  )
            }
          >
            <Plus size={12} />
          </Button>
        )}
      </div>

      <CreateDatabaseModal
        isOpen={createModalOpen}
        onClose={() => setCreateModalOpen(false)}
        sessionId={sessionId}
        driver={driver}
        environment={connection?.environment || 'development'}
        readOnly={connection?.read_only || false}
        connectionName={connection?.name}
        connectionDatabase={connection?.database}
        onCreated={() => {
          // Invalidate cache before refresh
          schemaCache.invalidateNamespaces();
          loadNamespaces();
        }}
      />
      {namespaces.map(ns => {
        const key = getNsKey(ns);
        const isExpanded = expandedNs === key;

        return (
          <div key={key}>
            <DatabaseContextMenu
              onOpen={() => openNamespace(ns)}
              onRefresh={() => refreshCollections(ns)}
              onCreateTable={() => {
                setCreateTableNamespace(ns);
                setCreateTableOpen(true);
              }}
              onDelete={() => {
                setDeleteTargetNamespace(ns);
                setDeleteModalOpen(true);
              }}
              canCreateTable={driverMeta.supportsSQL && !connection?.read_only}
              canDelete={!connection?.read_only}
            >
              <button
                type="button"
                className={cn(
                  'flex items-center gap-2 w-full px-2 py-1.5 rounded-md hover:bg-accent/10 transition-colors text-left',
                  isExpanded ? 'text-foreground' : 'text-muted-foreground'
                )}
                onClick={() => {
                  handleExpandNamespace(ns);
                  onDatabaseSelect?.(ns);
                }}
              >
                <span className="shrink-0">
                  {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                </span>
                <span
                  className={cn(
                    'shrink-0',
                    isExpanded ? 'text-accent' : 'text-muted-foreground/70'
                  )}
                >
                  <Database size={14} />
                </span>
                <span className="truncate">
                  {ns.schema ? `${ns.database}.${ns.schema}` : ns.database}
                </span>
              </button>
            </DatabaseContextMenu>

            {isExpanded && (
              <div className="flex flex-col ml-2 pl-2 border-l border-border mt-0.5 space-y-0.5">
                <div className="px-2 mb-2 pb-1.5 relative border-b border-border/50">
                  <Search
                    size={12}
                    className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground z-10"
                  />
                  <Input
                    className="h-7 text-xs pl-7 bg-muted/50 border-transparent focus-visible:bg-background shadow-none"
                    placeholder={t('browser.searchPlaceholder', {
                      label: t(terminology.tablePluralLabel).toLowerCase(),
                    })}
                    value={searchValue}
                    onChange={e => setSearchValue(e.target.value)}
                    onClick={e => e.stopPropagation()}
                  />
                </div>

                {(() => {
                  const tables = collections.filter(
                    c => c.collection_type === 'Table' || c.collection_type === 'Collection'
                  );
                  if (tables.length === 0 && !collectionsLoading) return null;
                  return (
                    <div className="space-y-0.5">
                      <button
                        type="button"
                        className="flex items-center gap-1 px-2 py-0.5 text-xs text-muted-foreground hover:text-foreground w-full text-left"
                        onClick={() => toggleSection('tables')}
                      >
                        {expandedSections.has('tables') ? (
                          <ChevronDown size={12} />
                        ) : (
                          <ChevronRight size={12} />
                        )}
                        <Table size={12} />
                        <span>{t('dbtree.tables')}</span>
                        <span className="text-muted-foreground/60 ml-auto">{tables.length}</span>
                      </button>
                      {expandedSections.has('tables') &&
                        tables.map(col => (
                          <TableContextMenu
                            key={col.name}
                            collection={col}
                            sessionId={sessionId}
                            connectionId={connection?.id}
                            driver={driver as Driver}
                            environment={connection?.environment || 'development'}
                            readOnly={connection?.read_only || false}
                            onRefresh={() => refreshCollections(col.namespace)}
                            onOpen={() => handleTableClick(col)}
                            onCompareWith={onCompareTable}
                            onAiGenerate={onAiGenerateForTable}
                          >
                            <button
                              type="button"
                              className="flex items-center gap-2 w-full px-2 py-1.5 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground text-left group ml-5"
                              onClick={() => handleTableClick(col)}
                            >
                              <span className="shrink-0 group-hover:text-foreground/80 transition-colors">
                                <Table size={13} />
                              </span>
                              <span className="truncate font-mono text-xs">{col.name}</span>
                            </button>
                          </TableContextMenu>
                        ))}
                    </div>
                  );
                })()}

                {/* Views Section */}
                {(() => {
                  const views = collections.filter(c => c.collection_type === 'View');
                  if (views.length === 0) return null;
                  return (
                    <div className="space-y-0.5 mt-2">
                      <button
                        type="button"
                        className="flex items-center gap-1 px-2 py-0.5 text-xs text-muted-foreground hover:text-foreground w-full text-left"
                        onClick={() => toggleSection('views')}
                      >
                        {expandedSections.has('views') ? (
                          <ChevronDown size={12} />
                        ) : (
                          <ChevronRight size={12} />
                        )}
                        <Eye size={12} />
                        <span>{t('dbtree.views')}</span>
                        <span className="text-muted-foreground/60 ml-auto">{views.length}</span>
                      </button>
                      {expandedSections.has('views') &&
                        views.map(col => (
                          <TableContextMenu
                            key={col.name}
                            collection={col}
                            sessionId={sessionId}
                            connectionId={connection?.id}
                            driver={driver as Driver}
                            environment={connection?.environment || 'development'}
                            readOnly={connection?.read_only || false}
                            onRefresh={() => refreshCollections(col.namespace)}
                            onOpen={() => handleTableClick(col)}
                            onCompareWith={onCompareTable}
                            onAiGenerate={onAiGenerateForTable}
                          >
                            <button
                              type="button"
                              className="flex items-center gap-2 w-full px-2 py-1.5 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground text-left group ml-5"
                              onClick={() => handleTableClick(col)}
                            >
                              <span className="shrink-0 group-hover:text-foreground/80 transition-colors">
                                <Eye size={13} />
                              </span>
                              <span className="truncate font-mono text-xs">{col.name}</span>
                            </button>
                          </TableContextMenu>
                        ))}
                    </div>
                  );
                })()}

                {/* Materialized Views Section (PostgreSQL) */}
                {(() => {
                  const matViews = collections.filter(
                    c => c.collection_type === 'MaterializedView'
                  );
                  if (matViews.length === 0) return null;
                  return (
                    <div className="space-y-0.5 mt-2">
                      <button
                        type="button"
                        className="flex items-center gap-1 px-2 py-0.5 text-xs text-muted-foreground hover:text-foreground w-full text-left"
                        onClick={() => toggleSection('materializedViews')}
                      >
                        {expandedSections.has('materializedViews') ? (
                          <ChevronDown size={12} />
                        ) : (
                          <ChevronRight size={12} />
                        )}
                        <Layers size={12} />
                        <span>{t('dbtree.materializedViews')}</span>
                        <span className="text-muted-foreground/60 ml-auto">{matViews.length}</span>
                      </button>
                      {expandedSections.has('materializedViews') &&
                        matViews.map(col => (
                          <TableContextMenu
                            key={col.name}
                            collection={col}
                            sessionId={sessionId}
                            connectionId={connection?.id}
                            driver={driver as Driver}
                            environment={connection?.environment || 'development'}
                            readOnly={connection?.read_only || false}
                            onRefresh={() => refreshCollections(col.namespace)}
                            onOpen={() => handleTableClick(col)}
                            onCompareWith={onCompareTable}
                            onAiGenerate={onAiGenerateForTable}
                          >
                            <button
                              type="button"
                              className="flex items-center gap-2 w-full px-2 py-1.5 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground text-left group ml-5"
                              onClick={() => handleTableClick(col)}
                            >
                              <span className="shrink-0 group-hover:text-foreground/80 transition-colors">
                                <Layers size={13} />
                              </span>
                              <span className="truncate font-mono text-xs">{col.name}</span>
                            </button>
                          </TableContextMenu>
                        ))}
                    </div>
                  );
                })()}

                {/* Functions Section */}
                {(() => {
                  const functions = routines.filter(r => r.routine_type === 'Function');
                  if (functions.length === 0 && !routinesLoading) return null;
                  return (
                    <div className="space-y-0.5 mt-2">
                      <button
                        type="button"
                        className="flex items-center gap-1 px-2 py-0.5 text-xs text-muted-foreground hover:text-foreground w-full text-left"
                        onClick={() => toggleSection('functions')}
                      >
                        {expandedSections.has('functions') ? (
                          <ChevronDown size={12} />
                        ) : (
                          <ChevronRight size={12} />
                        )}
                        <FunctionSquare size={12} />
                        <span>{t('dbtree.functions')}</span>
                        <span className="text-muted-foreground/60 ml-auto">{functions.length}</span>
                      </button>
                      {expandedSections.has('functions') &&
                        functions.map(routine => (
                          <div
                            key={routine.name}
                            className="flex items-center gap-2 w-full px-2 py-1.5 rounded-md text-muted-foreground text-left group ml-5"
                            title={`${routine.name}(${routine.arguments})${routine.return_type ? ` → ${routine.return_type}` : ''}`}
                          >
                            <span className="shrink-0">
                              <FunctionSquare size={13} />
                            </span>
                            <span className="truncate font-mono text-xs">{routine.name}</span>
                            {routine.language && (
                              <span className="text-[10px] text-muted-foreground/60 ml-auto">
                                {routine.language}
                              </span>
                            )}
                          </div>
                        ))}
                    </div>
                  );
                })()}

                {/* Procedures Section */}
                {(() => {
                  const procedures = routines.filter(r => r.routine_type === 'Procedure');
                  if (procedures.length === 0 && !routinesLoading) return null;
                  return (
                    <div className="space-y-0.5 mt-2">
                      <button
                        type="button"
                        className="flex items-center gap-1 px-2 py-0.5 text-xs text-muted-foreground hover:text-foreground w-full text-left"
                        onClick={() => toggleSection('procedures')}
                      >
                        {expandedSections.has('procedures') ? (
                          <ChevronDown size={12} />
                        ) : (
                          <ChevronRight size={12} />
                        )}
                        <PlayCircle size={12} />
                        <span>{t('dbtree.procedures')}</span>
                        <span className="text-muted-foreground/60 ml-auto">
                          {procedures.length}
                        </span>
                      </button>
                      {expandedSections.has('procedures') &&
                        procedures.map(routine => (
                          <div
                            key={routine.name}
                            className="flex items-center gap-2 w-full px-2 py-1.5 rounded-md text-muted-foreground text-left group ml-5"
                            title={`${routine.name}(${routine.arguments})`}
                          >
                            <span className="shrink-0">
                              <PlayCircle size={13} />
                            </span>
                            <span className="truncate font-mono text-xs">{routine.name}</span>
                            {routine.language && (
                              <span className="text-[10px] text-muted-foreground/60 ml-auto">
                                {routine.language}
                              </span>
                            )}
                          </div>
                        ))}
                    </div>
                  );
                })()}

                {/* Triggers Section */}
                {(() => {
                  if (triggers.length === 0 && !triggersLoading) return null;
                  return (
                    <div className="space-y-0.5 mt-2">
                      <button
                        type="button"
                        className="flex items-center gap-1 px-2 py-0.5 text-xs text-muted-foreground hover:text-foreground w-full text-left"
                        onClick={() => toggleSection('triggers')}
                      >
                        {expandedSections.has('triggers') ? (
                          <ChevronDown size={12} />
                        ) : (
                          <ChevronRight size={12} />
                        )}
                        <Zap size={12} />
                        <span>{t('dbtree.triggers')}</span>
                        <span className="text-muted-foreground/60 ml-auto">{triggers.length}</span>
                      </button>
                      {expandedSections.has('triggers') &&
                        triggers.map(trigger => (
                          <div
                            key={trigger.name}
                            className="flex items-center gap-2 w-full px-2 py-1.5 rounded-md text-muted-foreground text-left group ml-5"
                            title={`${trigger.timing} ${trigger.events.join(' | ')} ON ${trigger.table_name}`}
                          >
                            <span className="shrink-0">
                              <Zap size={13} className={cn(!trigger.enabled && 'opacity-40')} />
                            </span>
                            <span className="truncate font-mono text-xs">{trigger.name}</span>
                            <span className="text-[10px] text-muted-foreground/60 ml-auto">
                              {trigger.table_name}
                            </span>
                          </div>
                        ))}
                    </div>
                  );
                })()}

                {/* Events Section (MySQL only) */}
                {(() => {
                  if (dbEvents.length === 0) return null;
                  return (
                    <div className="space-y-0.5 mt-2">
                      <button
                        type="button"
                        className="flex items-center gap-1 px-2 py-0.5 text-xs text-muted-foreground hover:text-foreground w-full text-left"
                        onClick={() => toggleSection('events')}
                      >
                        {expandedSections.has('events') ? (
                          <ChevronDown size={12} />
                        ) : (
                          <ChevronRight size={12} />
                        )}
                        <Calendar size={12} />
                        <span>{t('dbtree.events')}</span>
                        <span className="text-muted-foreground/60 ml-auto">{dbEvents.length}</span>
                      </button>
                      {expandedSections.has('events') &&
                        dbEvents.map(evt => (
                          <div
                            key={evt.name}
                            className="flex items-center gap-2 w-full px-2 py-1.5 rounded-md text-muted-foreground text-left group ml-5"
                            title={`${evt.event_type}${evt.interval_value ? ` every ${evt.interval_value} ${evt.interval_field}` : ''}`}
                          >
                            <span className="shrink-0">
                              <Calendar size={13} />
                            </span>
                            <span className="truncate font-mono text-xs">{evt.name}</span>
                            <span
                              className={cn(
                                'text-[10px] ml-auto',
                                evt.status === 'Enabled'
                                  ? 'text-emerald-600'
                                  : 'text-muted-foreground/60'
                              )}
                            >
                              {evt.status === 'Enabled' ? '' : evt.status}
                            </span>
                          </div>
                        ))}
                    </div>
                  );
                })()}

                {/* Load More for collections */}
                {canLoadMore && !collectionsLoading && (
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 justify-start px-2 text-xs text-muted-foreground hover:text-foreground w-full"
                    onClick={handleLoadMore}
                  >
                    {t('dbtree.loadMore')} ({collectionsTotal - collections.length})
                  </Button>
                )}

                {/* Empty state */}
                {collections.length === 0 &&
                  routines.length === 0 &&
                  triggers.length === 0 &&
                  !collectionsLoading &&
                  !routinesLoading &&
                  !triggersLoading && (
                    <div className="px-2 py-1 text-xs text-muted-foreground italic">
                      {search ? t('common.noResults') : t('common.noResults')}
                    </div>
                  )}
              </div>
            )}
          </div>
        );
      })}

      {createTableNamespace && (
        <CreateTableModal
          isOpen={createTableOpen}
          onClose={() => {
            setCreateTableOpen(false);
            setCreateTableNamespace(null);
          }}
          sessionId={sessionId}
          namespace={createTableNamespace}
          driver={driver as Driver}
          onTableCreated={tableName => {
            if (!createTableNamespace) return;
            // Invalidate cache before refresh
            schemaCache.invalidateCollections(createTableNamespace);
            refreshCollections(createTableNamespace);
            if (tableName) {
              emitTableChange({
                type: 'create',
                namespace: createTableNamespace,
                tableName,
              });
            }
          }}
        />
      )}

      {deleteTargetNamespace && (
        <DeleteDatabaseModal
          isOpen={deleteModalOpen}
          onClose={() => {
            setDeleteModalOpen(false);
            setDeleteTargetNamespace(null);
          }}
          sessionId={sessionId}
          namespace={deleteTargetNamespace}
          driver={driver}
          environment={connection?.environment || 'development'}
          onDeleted={() => {
            schemaCache.invalidateNamespaces();
            loadNamespaces();
            // Clear expanded state if we deleted the expanded namespace
            if (expandedNs && getNsKey(deleteTargetNamespace) === expandedNs) {
              setExpandedNs(null);
              setExpandedNamespace(null);
              setCollections([]);
              setCollectionsTotal(0);
            }
          }}
        />
      )}
    </div>
  );
}
