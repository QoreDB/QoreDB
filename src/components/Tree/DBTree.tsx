import { useState, useEffect, useCallback } from 'react';
import { Namespace, Collection, SavedConnection, listCollections, RelationFilter } from '../../lib/tauri';
import { useSchemaCache } from '../../hooks/useSchemaCache';
import { Folder, FolderOpen, Table, Eye, Loader2, Plus, ChevronRight, ChevronDown, Search } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { CreateDatabaseModal } from './CreateDatabaseModal';
import { DeleteDatabaseModal } from './DeleteDatabaseModal';
import { TableContextMenu } from './TableContextMenu';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Driver, getDriverMetadata } from '../../lib/drivers';
import { getTerminology } from '../../lib/driverCapabilities';
import { CreateTableModal } from '../Table/CreateTableModal';
import { DatabaseContextMenu } from './DatabaseContextMenu';
import { emitTableChange } from '@/lib/tableEvents';

interface DBTreeProps {
  connectionId: string;
  driver: string;
  connection?: SavedConnection;
  onTableSelect?: (namespace: Namespace, tableName: string, relationFilter?: RelationFilter) => void;
  onDatabaseSelect?: (namespace: Namespace) => void;
  refreshTrigger?: number;
  activeNamespace?: Namespace | null;
}

export function DBTree({
  connectionId,
  driver,
  connection,
  onTableSelect,
  onDatabaseSelect,
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
  const schemaCache = useSchemaCache(connectionId);
  const [createModalOpen, setCreateModalOpen] = useState(false);
  const [createTableOpen, setCreateTableOpen] = useState(false);
  const [createTableNamespace, setCreateTableNamespace] = useState<Namespace | null>(null);
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [deleteTargetNamespace, setDeleteTargetNamespace] = useState<Namespace | null>(null);
  const [search, setSearch] = useState("");
  const [searchValue, setSearchValue] = useState("");  
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
    [connectionId, collectionsPageSize, search]
  );

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
  }, [activeNamespace, expandedNs, refreshCollections]);

  const canLoadMore = collections.length > 0 && collections.length < collectionsTotal;

  // Reload when search changes
  useEffect(() => {
    if (expandedNamespace) {
      refreshCollections(expandedNamespace, 1, false);
    }
  }, [search, expandedNamespace, refreshCollections]);

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
  }, [connectionId, loadNamespaces]);

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
      setSearch("");
      setSearchValue("");
      if (activeNamespace && getNsKey(activeNamespace) === key) {
        setCollapsedActiveNsKey(key);
      }
      return;
    }

    setExpandedNs(key);
    setExpandedNamespace(ns);
    setCollapsedActiveNsKey(null);
    setSearch("");
    setSearchValue("");
    await refreshCollections(ns, 1, false);
  }

  async function openNamespace(ns: Namespace) {
    const key = getNsKey(ns);
    if (expandedNs !== key) {
      setExpandedNs(key);
      setExpandedNamespace(ns);
      setCollapsedActiveNsKey(null);
      await refreshCollections(ns, 1, false);
    }
    onDatabaseSelect?.(ns);
  }

  function handleTableClick(col: Collection) {
    onTableSelect?.(col.namespace, col.name);
  }

  function getNsKey(ns: Namespace): string {
    return `${ns.database}:${ns.schema || ''}`;
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
      <div className="flex items-center justify-between px-2 py-1 mb-1">
         <span className="text-xs font-semibold text-muted-foreground">
           {t(driverMeta.treeRootLabel)}
         </span>
         {driverMeta.createAction !== 'none' && (
           <Button 
              variant="ghost" 
              size="icon" 
              className="h-5 w-5" 
              onClick={() => setCreateModalOpen(true)}
              disabled={connection?.read_only}
              title={connection?.read_only ? t('environment.blocked') : t(driverMeta.createAction === 'schema' ? 'database.newSchema' : 'database.newDatabase')}
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
                className={cn(
                  "flex items-center gap-2 w-full px-2 py-1.5 rounded-md hover:bg-accent/10 transition-colors text-left",
                  isExpanded ? "text-foreground" : "text-muted-foreground"
                )}
                onClick={() => {
                  handleExpandNamespace(ns);
                  onDatabaseSelect?.(ns);
                }}
              >
                <span className="shrink-0">
                  {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                </span>
                <span className="shrink-0">
                  {isExpanded ? <FolderOpen size={14} /> : <Folder size={14} />}
                </span>
                <span className="truncate">
                  {ns.schema ? `${ns.database}.${ns.schema}` : ns.database}
                </span>
              </button>
            </DatabaseContextMenu>
            
            {isExpanded && (
              <div className="flex flex-col ml-2 pl-2 border-l border-border mt-0.5 space-y-0.5">
                <div className="px-2 mb-1 relative">
                   <Search size={12} className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground z-10" />
                   <Input
                      className="h-7 text-xs pl-7 bg-muted/50 border-transparent focus-visible:bg-background shadow-none"
                      placeholder={t('browser.searchPlaceholder', { label: t(terminology.tablePluralLabel).toLowerCase() })}
                      value={searchValue}
                      onChange={(e) => setSearchValue(e.target.value)}
                      onClick={(e) => e.stopPropagation()}
                   />
                </div>
                {collections.length === 0 ? (
                  <div className="px-2 py-1 text-xs text-muted-foreground italic">
                    {search ? t('common.noResults') : t('common.noResults')}
                  </div>
                ) : (
                  <>
                  {collections.map(col => (
                    <TableContextMenu
                      key={col.name}
                      collection={col}
                      sessionId={sessionId}
                      driver={driver as Driver}
                      environment={connection?.environment || 'development'}
                      readOnly={connection?.read_only || false}
                      onRefresh={() => refreshCollections(col.namespace)}
                      onOpen={() => handleTableClick(col)}
                    >
                      <button
                        className="flex items-center gap-2 w-full px-2 py-1 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground text-left group"
                        onClick={() => handleTableClick(col)}
                      >
                        <span className="shrink-0 group-hover:text-foreground/80 transition-colors">
                          {col.collection_type === 'View' ? <Eye size={13} /> : <Table size={13} />}
                        </span>
                        <span className="truncate font-mono text-xs">{col.name}</span>
                      </button>
                    </TableContextMenu>
                  ))}
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
                  </>
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
          onTableCreated={(tableName) => {
            if (!createTableNamespace) return;
            // Invalidate cache before refresh
            schemaCache.invalidateCollections(createTableNamespace);
            refreshCollections(createTableNamespace);
            if (tableName) {
              emitTableChange({ type: 'create', namespace: createTableNamespace, tableName });
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
