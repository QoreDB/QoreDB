import { useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Copy, Pencil, Trash2, Search } from 'lucide-react';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { JSONViewer } from './JSONViewer';
import { DeleteConfirmDialog } from '../Grid/DeleteConfirmDialog';
import { QueryResult, Value, RowData as TauriRowData, deleteRow, Environment } from '@/lib/tauri';
import { cn } from '@/lib/utils';

interface DocumentResultsProps {
  result: QueryResult;
  sessionId?: string;
  database?: string;
  collection?: string;
  environment?: Environment;
  readOnly?: boolean;
  connectionName?: string;
  connectionDatabase?: string;
  onEditDocument?: (doc: Record<string, unknown>, idValue?: Value) => void;
  onRowsDeleted?: () => void;
}

type DocumentRow = {
  doc: Record<string, unknown> | unknown;
  idValue?: Value;
  idLabel?: string;
  json: string;
  search: string;
};

const DOCUMENT_COLUMN = 'document';

function coerceIdValue(id: unknown): Value | undefined {
  if (id && typeof id === 'object' && !Array.isArray(id)) {
    const oid = (id as Record<string, unknown>).$oid;
    if (typeof oid === 'string') {
      return oid;
    }
  }
  if (
    id === null ||
    typeof id === 'string' ||
    typeof id === 'number' ||
    typeof id === 'boolean' ||
    typeof id === 'object'
  ) {
    return id as Value;
  }
  return undefined;
}

function formatIdLabel(id: unknown): string {
  if (id === undefined) return '-';
  if (typeof id === 'string' || typeof id === 'number' || typeof id === 'boolean') {
    return String(id);
  }
  if (id && typeof id === 'object' && !Array.isArray(id)) {
    const oid = (id as Record<string, unknown>).$oid;
    if (typeof oid === 'string') return oid;
  }
  return JSON.stringify(id);
}

function normalizeDocument(
  result: QueryResult,
  rowValues: Value[],
): Record<string, unknown> | unknown {
  if (result.columns.length === 1 && result.columns[0]?.name === DOCUMENT_COLUMN) {
    return rowValues[0] ?? {};
  }

  const data: Record<string, unknown> = {};
  result.columns.forEach((col, idx) => {
    data[col.name] = rowValues[idx];
  });
  return data;
}

export function DocumentResults({
  result,
  sessionId,
  database,
  collection,
  environment = 'development',
  readOnly = false,
  connectionName,
  connectionDatabase,
  onEditDocument,
  onRowsDeleted,
}: DocumentResultsProps) {
  const { t } = useTranslation();
  const [filter, setFilter] = useState('');
  const [pendingDelete, setPendingDelete] = useState<DocumentRow | null>(null);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState('');
  const [isDeleting, setIsDeleting] = useState(false);

  const confirmationLabel =
    (connectionDatabase || connectionName || 'PROD').trim() || 'PROD';
  const requiresConfirm = environment === 'production';

  const documents = useMemo<DocumentRow[]>(() => {
    return result.rows.map((row) => {
      const doc = normalizeDocument(result, row.values);
      const json = JSON.stringify(doc ?? null, null, 2);
      const search = json.toLowerCase();
      const idRaw =
        doc && typeof doc === 'object' && !Array.isArray(doc)
          ? (doc as Record<string, unknown>)._id
          : undefined;
      const idValue = coerceIdValue(idRaw);
      const idLabel = formatIdLabel(idRaw);

      return {
        doc,
        idValue,
        idLabel,
        json,
        search,
      };
    });
  }, [result]);

  const filteredDocs = useMemo(() => {
    const query = filter.trim().toLowerCase();
    if (!query) return documents;
    return documents.filter((doc) => doc.search.includes(query));
  }, [documents, filter]);

  const parentRef = useRef<HTMLDivElement>(null);
  const rowVirtualizer = useVirtualizer({
    count: filteredDocs.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 180,
    overscan: 8,
  });

  const handleCopy = async (row: DocumentRow) => {
    await navigator.clipboard.writeText(row.json);
  };

  const handleDeleteClick = (row: DocumentRow) => {
    if (readOnly) {
      toast.error(t('environment.blocked'));
      return;
    }
    if (!sessionId || !database || !collection) {
      toast.error(t('common.error'));
      return;
    }
    setPendingDelete(row);
    setDeleteConfirmValue('');
    setDeleteDialogOpen(true);
  };

  const performDelete = async () => {
    if (!pendingDelete || !sessionId || !database || !collection) return;
    if (pendingDelete.idValue === undefined) {
      toast.error(t('grid.previewMissingPk'));
      return;
    }

    const pkData: TauriRowData = { columns: { _id: pendingDelete.idValue } };
    setIsDeleting(true);

    try {
      const res = await deleteRow(
        sessionId,
        database,
        '',
        collection,
        pkData,
      );
      if (res.success) {
        toast.success(t('grid.deleteSuccess', { count: 1 }));
        onRowsDeleted?.();
      } else {
        toast.error(res.error || t('grid.deleteError'));
      }
    } catch {
      toast.error(t('grid.deleteError'));
    } finally {
      setIsDeleting(false);
    }
  };

  const totalTimeMs = (result as { total_time_ms?: number }).total_time_ms;

  if (filteredDocs.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
        {t('grid.noResults')}
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full min-h-0 gap-3">
      <div className="flex items-center justify-between px-1 gap-3">
        <div className="flex items-center gap-3 text-xs text-muted-foreground">
          <span>{t('grid.rowsTotal', { count: filteredDocs.length })}</span>
          {typeof result.execution_time_ms === 'number' && (
            <div className="flex items-center gap-2 border-l border-border pl-3">
              <span title={t('query.time.execTooltip')}>
                {t('query.time.exec')}:{" "}
                <span className="font-mono text-foreground font-medium">
                  {result.execution_time_ms.toFixed(2)}ms
                </span>
              </span>
              {totalTimeMs !== undefined && (
                <>
                  <span className="text-border/50">|</span>
                  <span title={t('query.time.totalTooltip')}>
                    {t('query.time.total')}:{" "}
                    <span className="font-mono text-foreground font-bold">
                      {totalTimeMs.toFixed(2)}ms
                    </span>
                  </span>
                </>
              )}
            </div>
          )}
        </div>

        <div className="relative w-64">
          <Search
            size={14}
            className="absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground"
          />
          <Input
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            placeholder={t('grid.searchPlaceholder')}
            className="h-8 pl-7 text-xs"
          />
        </div>
      </div>

      <div
        ref={parentRef}
        className="flex-1 min-h-0 overflow-auto border border-border rounded-md bg-background"
      >
        <div
          className="relative"
          style={{ height: `${rowVirtualizer.getTotalSize()}px` }}
        >
          {rowVirtualizer.getVirtualItems().map((virtualRow) => {
            const doc = filteredDocs[virtualRow.index];
            return (
              <div
                key={virtualRow.key}
                className="absolute left-0 right-0 px-3 py-3"
                style={{
                  transform: `translateY(${virtualRow.start}px)`,
                }}
              >
                <div className="rounded-md border border-border bg-muted/10 shadow-sm">
                  <div className="flex items-center justify-between gap-3 px-3 py-2 border-b border-border">
                    <div className="flex items-center gap-2">
                      <span className="text-xs text-muted-foreground">
                        _id:
                      </span>
                      <span className="font-mono text-xs text-foreground truncate max-w-[240px]">
                        {doc.idLabel}
                      </span>
                    </div>
                    <div className="flex items-center gap-2">
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-7 px-2 text-xs"
                        onClick={() => handleCopy(doc)}
                        title={t('grid.copyToClipboard')}
                      >
                        <Copy size={12} className="mr-1" />
                        {t('grid.copyJSON')}
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        className={cn('h-7 px-2 text-xs', readOnly && 'opacity-50')}
                        onClick={() => {
                          if (readOnly) {
                            toast.error(t('environment.blocked'));
                            return;
                          }
                          if (doc.doc && typeof doc.doc === 'object' && !Array.isArray(doc.doc)) {
                            onEditDocument?.(doc.doc as Record<string, unknown>, doc.idValue);
                          }
                        }}
                        disabled={readOnly}
                        title={t('document.edit')}
                      >
                        <Pencil size={12} className="mr-1" />
                        {t('document.edit')}
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        className={cn(
                          'h-7 px-2 text-xs text-destructive hover:text-destructive',
                          readOnly && 'opacity-50'
                        )}
                        onClick={() => handleDeleteClick(doc)}
                        disabled={readOnly}
                        title={t('common.delete')}
                      >
                        <Trash2 size={12} className="mr-1" />
                        {t('common.delete')}
                      </Button>
                    </div>
                  </div>
                  <div className="max-h-96 overflow-auto">
                    <JSONViewer data={doc.doc ?? null} initialExpanded={false} maxDepth={6} />
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      <DeleteConfirmDialog
        open={deleteDialogOpen}
        onOpenChange={setDeleteDialogOpen}
        selectedCount={pendingDelete ? 1 : 0}
        previewRows={[
          {
            index: 1,
            values: pendingDelete?.idValue !== undefined
              ? [{ key: '_id', value: pendingDelete.idValue }]
              : [],
            hasMissing: pendingDelete?.idValue === undefined,
          },
        ]}
        totalSelectedRows={1}
        requiresConfirm={requiresConfirm}
        confirmLabel={confirmationLabel}
        confirmValue={deleteConfirmValue}
        onConfirmValueChange={setDeleteConfirmValue}
        onConfirm={async () => {
          await performDelete();
          setDeleteDialogOpen(false);
        }}
        isDeleting={isDeleting}
      />
    </div>
  );
}
