import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  Namespace, 
  TableSchema, 
  QueryResult,
  describeTable, 
  previewTable 
} from '../../lib/tauri';
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
  Plus
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Value } from '../../lib/tauri';
import { RowModal } from './RowModal'

interface TableBrowserProps {
  sessionId: string;
  namespace: Namespace;
  tableName: string;
  onClose: () => void;
}

type Tab = 'structure' | 'data';

export function TableBrowser({ 
  sessionId, 
  namespace, 
  tableName, 
  onClose 
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

  useEffect(() => {
    loadData();
  }, [sessionId, namespace, tableName]);

  async function loadData() {
    setLoading(true);
    setError(null);

    try {
      const startTime = performance.now();
      // Load both schema and preview in parallel
      const [schemaResult, dataResult] = await Promise.all([
        describeTable(sessionId, namespace, tableName),
        previewTable(sessionId, namespace, tableName, 100)
      ]);
      const endTime = performance.now();
      const totalTime = endTime - startTime;

      if (schemaResult.success && schemaResult.schema) {
        setSchema(schemaResult.schema);
      } else if (schemaResult.error) {
        setError(schemaResult.error);
      }

      if (dataResult.success && dataResult.result) {
        setData({
          ...dataResult.result,
          total_time_ms: totalTime
        } as QueryResult & { total_time_ms: number });
      } else if (dataResult.error && !error) {
        setError(dataResult.error);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load table data');
    } finally {
      setLoading(false);
    }
  }

  const displayName = namespace.schema 
    ? `${namespace.schema}.${tableName}` 
    : tableName;

  return (
    <div className="flex flex-col h-full bg-background rounded-lg border border-border shadow-sm overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border bg-muted/20">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-md bg-accent/10 text-accent">
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
                  <span>~{schema.row_count_estimate.toLocaleString()} {t('table.rows')}</span>
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
            onClick={() => {
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
            "px-3 py-1.5 text-sm font-medium rounded-md transition-colors",
            activeTab === 'data' 
              ? "bg-accent text-accent-foreground" 
              : "text-muted-foreground hover:text-foreground hover:bg-muted"
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
            "px-3 py-1.5 text-sm font-medium rounded-md transition-colors",
            activeTab === 'structure' 
              ? "bg-accent text-accent-foreground" 
              : "text-muted-foreground hover:text-foreground hover:bg-muted"
          )}
          onClick={() => setActiveTab('structure')}
        >
          <span className="flex items-center gap-2">
            <Key size={14} />
            {t('table.structure')}
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
            onRowsDeleted={loadData}
            onRowClick={(row) => {
              setModalMode('update');
              setSelectedRow(row);
              setIsModalOpen(true);
            }}
          />
        ) : (
          <StructureTable schema={schema} />
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
          <div className="w-8 p-2 text-center text-muted-foreground text-xs">
            {idx + 1}
          </div>
          <div className="flex-1 p-2 font-mono flex items-center gap-2">
            {col.is_primary_key && (
              <Key size={12} className="text-warning shrink-0" />
            )}
            <span className={cn(col.is_primary_key && "font-semibold")}>
              {col.name}
            </span>
          </div>
          <div className="w-32 p-2 font-mono text-xs text-accent">
            {col.data_type}
          </div>
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
          <span className="font-mono font-medium">
            {schema.primary_key.join(', ')}
          </span>
        </div>
      )}
    </div>
  );
}
