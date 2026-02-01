import { useCallback, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Copy, Pencil, Trash2, Search, ChevronDown, ChevronUp, Check, Database } from 'lucide-react';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { JSONViewer } from './JSONViewer';
import { DeleteConfirmDialog } from '../Grid/DeleteConfirmDialog';
import { DataGridPagination } from '../Grid/DataGridPagination';
import {  RowData as TauriRowData, deleteRow,  } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { coerceIdValue, DocumentResultsProps, DocumentRow, DocumentRowItemProps, formatIdLabel, normalizeDocument } from '@/utils/document'
import { useStreamingExport } from '@/hooks/useStreamingExport';
import { StreamingExportDialog } from '@/components/Export/StreamingExportDialog';
import type { ExportConfig } from '@/lib/export';

function DocumentRowItem({
  virtualRow,
  doc,
  measureElement,
  readOnly,
  t,
  onCopy,
  onEdit,
  onDelete
}: DocumentRowItemProps) {
  const lineCount = doc.json.split('\n').length;
  const isLong = lineCount > 12; 
  const [expanded, setExpanded] = useState(false);
  const [isCopied, setIsCopied] = useState(false);

  const shouldShowToggle = isLong;

  return (
    <div
      key={virtualRow.key}
      data-index={virtualRow.index}
      ref={measureElement}
      className="absolute left-0 right-0 px-3 py-1"
      style={{
        transform: `translateY(${virtualRow.start}px)`,
      }}
    >
      <div className="rounded-md border border-border bg-muted/10 shadow-sm flex flex-col">
        <div className="flex items-center justify-between gap-3 px-3 py-1.5 border-b border-border bg-muted/20">
          <div className="flex items-center gap-2">
            <span className="text-xs text-muted-foreground">_id:</span>
            <span className="font-mono text-xs text-foreground truncate max-w-60">
              {doc.idLabel}
            </span>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              className={cn('h-6 px-2 text-xs', isCopied && 'text-green-500')}
              onClick={() => {
                onCopy(doc);
                setIsCopied(true);
                toast.success(t('grid.copySuccess'));
                setTimeout(() => setIsCopied(false), 2000);
              }}
              title={t('grid.copyToClipboard')}
            >
              {isCopied ? (
                <Check size={12} className="mr-1" />
              ) : (
                <Copy size={12} className="mr-1" />
              )}
              {isCopied ? t('common.copied') : t('grid.copyJSON')}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className={cn('h-6 px-2 text-xs', readOnly && 'opacity-50')}
              onClick={() => {
                if (readOnly) {
                  toast.error(t('environment.blocked'));
                  return;
                }
                if (doc.doc && typeof doc.doc === 'object' && !Array.isArray(doc.doc)) {
                  onEdit(doc.doc as Record<string, unknown>, doc.idValue);
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
                'h-6 px-2 text-xs text-destructive hover:text-destructive',
                readOnly && 'opacity-50'
              )}
              onClick={() => onDelete(doc)}
              disabled={readOnly}
              title={t('common.delete')}
            >
              <Trash2 size={12} className="mr-1" />
              {t('common.delete')}
            </Button>
          </div>
        </div>

        <div className="relative">
          <div
            className={cn(
              'overflow-hidden transition-all duration-200',
              !expanded && shouldShowToggle ? 'max-h-45' : 'h-auto'
            )}
          >
            <JSONViewer data={doc.doc ?? null} initialExpanded={true} maxDepth={2} />
          </div>

          {!expanded && shouldShowToggle && (
            <div className="absolute bottom-0 left-0 right-0 h-16 bg-linear-to-t from-background to-transparent pointer-events-none" />
          )}
        </div>

        {shouldShowToggle && (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setExpanded(!expanded)}
            className="h-6 w-full rounded-t-none border-t border-border/50 text-[10px] text-muted-foreground hover:bg-muted/30 hover:text-foreground"
          >
            {expanded ? (
              <ChevronUp size={12} className="mr-1" />
            ) : (
              <ChevronDown size={12} className="mr-1" />
            )}
            {expanded ? t('grid.showLess') : t('grid.showMore')}
          </Button>
        )}
      </div>
    </div>
  );
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
  exportQuery,
  exportNamespace,
  serverSideTotalRows,
  serverSidePage,
  serverSidePageSize,
  onServerPageChange,
  onServerPageSizeChange,
}: DocumentResultsProps) {
  const { t } = useTranslation();
  const [filter, setFilter] = useState('');
  const [pendingDelete, setPendingDelete] = useState<DocumentRow | null>(null);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState('');
  const [isDeleting, setIsDeleting] = useState(false);
  const { startStreamingExport } = useStreamingExport(sessionId);
  const [streamingDialogOpen, setStreamingDialogOpen] = useState(false);
  const canStreamExport = Boolean(sessionId && exportQuery);



  const confirmationLabel =
    (connectionDatabase || connectionName || 'PROD').trim() || 'PROD';
  const requiresConfirm = environment === 'production';
  const resolvedNamespace = exportNamespace ?? (database ? { database, schema: undefined } : undefined);

  const isServerSidePaginated = serverSideTotalRows !== undefined;
  const totalRows = isServerSidePaginated ? serverSideTotalRows : result.rows.length;

  const documents = useMemo<DocumentRow[]>(() => {
    const renderRows = result.rows;
    return renderRows.map((row) => {
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
    estimateSize: () => 250,
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

  const performDelete = async (acknowledgedDangerous = false) => {
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
        acknowledgedDangerous,
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

  const handleStreamingExportConfirm = useCallback(
    async (config: ExportConfig) => {
      const exportId = await startStreamingExport(config);
      if (exportId) {
        setStreamingDialogOpen(false);
      }
    },
    [startStreamingExport]
  );

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
          <span>{t('grid.rowsTotal', { count: totalRows })}</span>
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

        <div className="flex items-center gap-2">
          {canStreamExport && (
            <Button
              variant="outline"
              size="sm"
              className="h-8 px-2 text-xs"
              onClick={() => setStreamingDialogOpen(true)}
            >
              <Database size={14} className="mr-1" />
              {t('grid.exportAllRows')}
            </Button>
          )}
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
      </div>

      <div
        ref={parentRef}
        className="flex-1 min-h-0 overflow-auto rounded-md bg-background"
      >
        <div
          className="relative"
          style={{ height: `${rowVirtualizer.getTotalSize()}px` }}
        >
          {rowVirtualizer.getVirtualItems().map((virtualRow) => (
             <DocumentRowItem
               key={virtualRow.key}
               virtualRow={virtualRow}
               doc={filteredDocs[virtualRow.index]}
               measureElement={rowVirtualizer.measureElement}
               readOnly={readOnly}
               t={t}
               onCopy={handleCopy}
               onEdit={onEditDocument || (() => {})}
               onDelete={handleDeleteClick}
             />
          ))}
        </div>
      </div>

      {isServerSidePaginated && (
        <DataGridPagination
          table={null}
          pagination={{ pageIndex: (serverSidePage || 1) - 1, pageSize: serverSidePageSize || 100 }}
          serverSideTotalRows={serverSideTotalRows}
          serverSidePage={serverSidePage}
          serverSidePageSize={serverSidePageSize}
          onServerPageChange={onServerPageChange}
          onServerPageSizeChange={onServerPageSizeChange}
        />
      )}

      {canStreamExport && exportQuery && (
        <StreamingExportDialog
          open={streamingDialogOpen}
          onOpenChange={setStreamingDialogOpen}
          query={exportQuery}
          namespace={resolvedNamespace}
          tableName={collection}
          onConfirm={handleStreamingExportConfirm}
        />
      )}

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
          await performDelete(true);
          setDeleteDialogOpen(false);
        }}
        isDeleting={isDeleting}
      />
    </div>
  );
}
