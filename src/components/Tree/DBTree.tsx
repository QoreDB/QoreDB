import { useState, useEffect } from 'react';
import { Namespace, Collection, listNamespaces, listCollections } from '../../lib/tauri';
import { Folder, FolderOpen, Table, Eye, Loader2, Plus } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { CreateDatabaseModal } from './CreateDatabaseModal';
import { useTranslation } from 'react-i18next';
import { getDriverMetadata } from '../../lib/drivers';

interface DBTreeProps {
  connectionId: string;
  driver: string;
  onTableSelect?: (namespace: Namespace, tableName: string) => void;
}

export function DBTree({ connectionId, driver, onTableSelect }: DBTreeProps) {
  const { t } = useTranslation();
  const [namespaces, setNamespaces] = useState<Namespace[]>([]);
  const [expandedNs, setExpandedNs] = useState<string | null>(null);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [loading, setLoading] = useState(false);
  const [createModalOpen, setCreateModalOpen] = useState(false);
  
  const driverMeta = getDriverMetadata(driver);

  // TODO: Get sessionId from connection
  const sessionId = connectionId;

  useEffect(() => {
    loadNamespaces();
  }, [connectionId]);

  async function loadNamespaces() {
    try {
      setLoading(true);
      const result = await listNamespaces(sessionId);
      if (result.success && result.namespaces) {
        setNamespaces(result.namespaces);
      }
    } catch (err) {
      console.error('Failed to load namespaces:', err);
    } finally {
      setLoading(false);
    }
  }

  async function handleExpandNamespace(ns: Namespace) {
    const key = `${ns.database}:${ns.schema || ''}`;
    
    if (expandedNs === key) {
      setExpandedNs(null);
      setCollections([]);
      return;
    }

    setExpandedNs(key);
    
    try {
      console.log('[DBTree] Calling listCollections with:', { sessionId, namespace: ns });
      const result = await listCollections(sessionId, ns);
      console.log('[DBTree] listCollections result:', result);
      if (result.success && result.collections) {
        console.log('[DBTree] Setting collections:', result.collections.length, 'items');
        setCollections(result.collections);
      } else {
        console.error('[DBTree] listCollections failed:', result.error);
      }
    } catch (err) {
      console.error('Failed to load collections:', err);
    }
  }

  function handleTableClick(col: Collection) {
    onTableSelect?.(col.namespace, col.name);
  }

  function getNsKey(ns: Namespace): string {
    return `${ns.database}:${ns.schema || ''}`;
  }

  if (loading) {
    return (
      <div className="flex items-center gap-2 p-2 text-sm text-muted-foreground animate-pulse">
        <Loader2 size={14} className="animate-spin" /> Loading...
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
              title={t(driverMeta.createAction === 'schema' ? 'database.newSchema' : 'database.newDatabase')}
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
        onCreated={loadNamespaces}
      />
      {namespaces.map(ns => {
        const key = getNsKey(ns);
        const isExpanded = expandedNs === key;
        
        return (
          <div key={key}>
            <button
              className={cn(
                "flex items-center gap-2 w-full px-2 py-1.5 rounded-md hover:bg-accent/10 transition-colors text-left",
                isExpanded ? "text-foreground" : "text-muted-foreground"
              )}
              onClick={() => handleExpandNamespace(ns)}
            >
              <span className="shrink-0">
                {isExpanded ? <FolderOpen size={14} /> : <Folder size={14} />}
              </span>
              <span className="truncate flex-1">
                {ns.schema ? `${ns.database}.${ns.schema}` : ns.database}
              </span>
            </button>
            
            {isExpanded && (
              <div className="flex flex-col ml-2 pl-2 border-l border-border mt-0.5 space-y-0.5">
                {collections.length === 0 ? (
                  <div className="px-2 py-1 text-xs text-muted-foreground italic">{t('dbtree.noCollections')}</div>
                ) : (
                  collections.map(col => (
                    <button
                      key={col.name}
                      className="flex items-center gap-2 px-2 py-1 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground text-left"
                      onClick={() => handleTableClick(col)}
                    >
                      <span className="shrink-0">
                        {col.collection_type === 'View' ? <Eye size={13} /> : <Table size={13} />}
                      </span>
                      <span className="truncate font-mono text-xs">{col.name}</span>
                    </button>
                  ))
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

