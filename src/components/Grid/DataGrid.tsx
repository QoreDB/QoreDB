// SPDX-License-Identifier: Apache-2.0

import {
  type ColumnDef,
  type ColumnFiltersState,
  createColumnHelper,
  getCoreRowModel,
  getFilteredRowModel,
  getPaginationRowModel,
  getSortedRowModel,
  type PaginationState,
  type RowSelectionState,
  type SortingState,
  useReactTable,
  type VisibilityState,
} from '@tanstack/react-table';
import { useVirtualizer } from '@tanstack/react-virtual';
import { CheckCircle2, Pencil } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { StreamingExportDialog } from '@/components/Export/StreamingExportDialog';
import { DangerConfirmDialog } from '@/components/Guard/DangerConfirmDialog';
import { Checkbox } from '@/components/ui/checkbox';
import { useStreamingExport } from '@/hooks/useStreamingExport';
import { aiExplainResult } from '@/lib/ai';
import type { ExportConfig } from '@/lib/export';
import { applyOverlay, emptyOverlayResult, type OverlayResult } from '@/lib/sandboxOverlay';
import type { SandboxChange, SandboxDeleteDisplay } from '@/lib/sandboxTypes';
import type {
  Environment,
  Namespace,
  QueryResult,
  RelationFilter,
  SortDirection,
  TableSchema,
  Value,
} from '@/lib/tauri';
import { type ExportDataDetail, UI_EVENT_EXPORT_DATA } from '@/lib/uiEvents';
import { useAiPreferences } from '@/providers/AiPreferencesProvider';
import { useLicense } from '@/providers/LicenseProvider';
import { DataGridColumnHeader } from './DataGridColumnHeader';
import { DataGridHeader } from './DataGridHeader';
import { DataGridPagination } from './DataGridPagination';
import { DataGridStatusBar } from './DataGridStatusBar';
import { DataGridTableBody } from './DataGridTableBody';
import { DataGridTableHeader } from './DataGridTableHeader';
import { DataGridToolbar } from './DataGridToolbar';
import { DeleteConfirmDialog } from './DeleteConfirmDialog';
import { EditableDataCell } from './EditableDataCell';
import { useDataGridCopy } from './hooks/useDataGridCopy';
import { useDataGridDelete } from './hooks/useDataGridDelete';
import { useDataGridExport } from './hooks/useDataGridExport';
import { useForeignKeyPeek } from './hooks/useForeignKeyPeek';
import { useInlineEdit } from './hooks/useInlineEdit';
import { convertToRowData, formatValue, type RowData } from './utils/dataGridUtils';

const EMPTY_OVERLAY_RESULT: OverlayResult = {
  result: {
    columns: [],
    rows: [],
    affected_rows: undefined,
    execution_time_ms: 0,
  },
  rowMetadata: new Map(),
  stats: {
    insertedRows: 0,
    modifiedRows: 0,
    deletedRows: 0,
    hiddenRows: 0,
  },
};

interface DataGridProps {
  result: QueryResult | null;
  height?: number;
  sessionId?: string;
  namespace?: Namespace;
  tableName?: string;
  tableSchema?: TableSchema | null;
  primaryKey?: string[];
  environment?: Environment;
  readOnly?: boolean;
  mutationsSupported?: boolean;
  connectionName?: string;
  connectionDatabase?: string;
  initialFilter?: string;
  onRowsDeleted?: () => void;
  onRowClick?: (row: RowData) => void;
  onRowsUpdated?: () => void;
  onOpenRelatedTable?: (
    namespace: Namespace,
    tableName: string,
    relationFilter?: RelationFilter
  ) => void;
  sandboxMode?: boolean;
  pendingChanges?: SandboxChange[];
  sandboxDeleteDisplay?: SandboxDeleteDisplay;
  onSandboxUpdate?: (
    primaryKey: Record<string, Value>,
    oldValues: Record<string, Value>,
    newValues: Record<string, Value>
  ) => void;
  onSandboxDelete?: (primaryKey: Record<string, Value>, oldValues: Record<string, Value>) => void;
  // Infinite scroll props
  infiniteScrollTotalRows?: number;
  infiniteScrollLoadedRows?: number;
  infiniteScrollIsFetchingMore?: boolean;
  infiniteScrollIsComplete?: boolean;
  onFetchMore?: () => void;
  // Server-side sort/search
  serverSortColumn?: string;
  serverSortDirection?: SortDirection;
  onServerSortChange?: (column?: string, direction?: SortDirection) => void;
  serverSearchTerm?: string;
  onServerSearchChange?: (term: string) => void;
  exportQuery?: string;
}

export function DataGrid({
  result,
  sessionId,
  namespace,
  tableName,
  tableSchema,
  primaryKey,
  environment = 'development',
  readOnly = false,
  mutationsSupported = true,
  connectionName,
  connectionDatabase,
  initialFilter,
  onRowsDeleted,
  onRowClick,
  onRowsUpdated,
  onOpenRelatedTable,
  sandboxMode = false,
  pendingChanges = [],
  sandboxDeleteDisplay = 'strikethrough',
  onSandboxUpdate,
  onSandboxDelete,
  infiniteScrollTotalRows,
  infiniteScrollLoadedRows,
  infiniteScrollIsFetchingMore,
  infiniteScrollIsComplete,
  onFetchMore,
  serverSortColumn,
  serverSortDirection,
  onServerSortChange,
  serverSearchTerm,
  onServerSearchChange,
  exportQuery,
}: DataGridProps) {
  const { t } = useTranslation();

  const [sorting, setSorting] = useState<SortingState>([]);
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({});
  const [pagination, setPagination] = useState<PaginationState>({
    pageIndex: 0,
    pageSize: 50,
  });
  const [internalGlobalFilter, setInternalGlobalFilter] = useState(initialFilter ?? '');
  const initialFilterRef = useRef<string | undefined>(undefined);
  const isInfiniteScrollMode = infiniteScrollTotalRows !== undefined;
  const isServerSideMode = isInfiniteScrollMode;
  const noopServerSearchChange = useCallback((_term: string) => {}, []);
  const globalFilter = isServerSideMode ? (serverSearchTerm ?? '') : internalGlobalFilter;
  const setGlobalFilter = isServerSideMode
    ? (onServerSearchChange ?? noopServerSearchChange)
    : setInternalGlobalFilter;

  const [columnVisibility, setColumnVisibility] = useState<VisibilityState>({});
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([]);
  const [showFilters, setShowFilters] = useState(false);
  const isServerSideSorting = isServerSideMode;

  useEffect(() => {
    if (!isServerSideSorting) return;

    if (!serverSortColumn || !serverSortDirection) {
      setSorting([]);
      return;
    }

    setSorting([
      {
        id: serverSortColumn,
        desc: serverSortDirection === 'desc',
      },
    ]);
  }, [isServerSideSorting, serverSortColumn, serverSortDirection]);

  const handleSortingChange = useCallback(
    (updater: SortingState | ((old: SortingState) => SortingState)) => {
      const nextSorting = typeof updater === 'function' ? updater(sorting) : updater;

      setSorting(nextSorting);
      if (!isServerSideSorting) return;

      const primarySort = nextSorting[0];
      if (!primarySort) {
        onServerSortChange?.(undefined, undefined);
        return;
      }

      onServerSortChange?.(primarySort.id, primarySort.desc ? 'desc' : 'asc');
    },
    [sorting, isServerSideSorting, onServerSortChange]
  );

  useEffect(() => {
    if (initialFilter === undefined) return;

    const previousInitial = initialFilterRef.current;
    if (previousInitial === undefined) {
      initialFilterRef.current = initialFilter;
      if (initialFilter !== globalFilter) {
        setGlobalFilter(initialFilter);
      }
      return;
    }

    if (previousInitial !== initialFilter) {
      if (globalFilter === previousInitial) {
        setGlobalFilter(initialFilter);
      }
      initialFilterRef.current = initialFilter;
    }
  }, [initialFilter, globalFilter, setGlobalFilter]);

  const searchInputRef = useRef<HTMLInputElement>(null);
  const parentRef = useRef<HTMLDivElement>(null);
  const confirmationLabel = (connectionDatabase || connectionName || 'PROD').trim() || 'PROD';

  const totalRows = result?.rows.length ?? 0;

  const overlayResult: OverlayResult = useMemo(() => {
    if (!result || !sandboxMode || pendingChanges.length === 0 || !namespace || !tableName) {
      return result ? emptyOverlayResult(result) : EMPTY_OVERLAY_RESULT;
    }
    return applyOverlay(result, pendingChanges, tableSchema ?? null, namespace, tableName, {
      deleteDisplay: sandboxDeleteDisplay,
      primaryKey,
    });
  }, [
    result,
    sandboxMode,
    pendingChanges,
    namespace,
    tableName,
    tableSchema,
    sandboxDeleteDisplay,
    primaryKey,
  ]);

  const data = useMemo(() => {
    const effectiveResult = sandboxMode ? overlayResult.result : result;
    if (!effectiveResult) return [];
    return convertToRowData({ ...effectiveResult });
  }, [result, overlayResult.result, sandboxMode]);

  const columnTypeMap = useMemo(() => {
    const map = new Map<string, string>();
    result?.columns.forEach(col => {
      map.set(col.name, col.data_type);
    });
    return map;
  }, [result]);

  const primaryKeySet = useMemo(() => {
    return new Set(primaryKey ?? []);
  }, [primaryKey]);

  const { indexedColumns, uniqueColumns, indexInfoMap } = useMemo(() => {
    const indexedColumns = new Set<string>();
    const uniqueColumns = new Set<string>();
    const indexInfoMap = new Map<string, { name: string; isComposite: boolean }>();

    if (tableSchema?.indexes) {
      for (const index of tableSchema.indexes) {
        if (index.is_primary) continue;

        const isComposite = index.columns.length > 1;

        for (const col of index.columns) {
          indexedColumns.add(col);

          if (index.is_unique) {
            uniqueColumns.add(col);
          }
          if (!indexInfoMap.has(col)) {
            indexInfoMap.set(col, { name: index.name, isComposite });
          }
        }
      }
    }

    return { indexedColumns, uniqueColumns, indexInfoMap };
  }, [tableSchema?.indexes]);

  const {
    peekCache,
    foreignKeyMap,
    buildPeekKey,
    ensurePeekLoaded,
    resolveReferencedNamespace,
    getRelationLabel,
  } = useForeignKeyPeek({
    sessionId,
    namespace,
    tableSchema,
  });

  const {
    setEditingValue,
    editInputRef,
    isUpdating,
    startInlineEdit,
    commitInlineEdit,
    cancelInlineEdit,
    inlineEditAvailable,
    editingCellRef,
    editingValueRef,
    updateConfirmOpen,
    setUpdateConfirmOpen,
    pendingUpdate,
    setPendingUpdate,
    performInlineUpdate,
  } = useInlineEdit({
    sessionId,
    namespace,
    tableName,
    primaryKey,
    environment,
    readOnly,
    mutationsSupported,
    sandboxMode,
    columnTypeMap,
    onSandboxUpdate,
    onRowsUpdated,
  });

  // Reset editing state when result changes
  useEffect(() => {
    cancelInlineEdit();
  }, [cancelInlineEdit]);

  // Build columns
  const columns = useMemo<ColumnDef<RowData, Value>[]>(() => {
    if (!result || result.columns.length === 0) return [];

    const columnHelper = createColumnHelper<RowData>();

    const actionColumn = onRowClick
      ? columnHelper.display({
          id: 'actions',
          header: () => <span className="sr-only">{t('grid.openRow')}</span>,
          cell: ({ row }) => (
            <div className="flex justify-center">
              <button
                type="button"
                className="h-6 w-6 inline-flex items-center justify-center rounded text-muted-foreground hover:text-foreground hover:bg-muted/80 transition-colors"
                onClick={event => {
                  event.stopPropagation();
                  onRowClick(row.original);
                }}
                aria-label={t('grid.openRow')}
                title={t('grid.openRow')}
              >
                <Pencil size={13} />
              </button>
            </div>
          ),
          size: 36,
        })
      : null;

    const selectColumn = columnHelper.display({
      id: 'select',
      header: ({ table }) => (
        <Checkbox
          checked={
            table.getIsAllRowsSelected()
              ? true
              : table.getIsSomeRowsSelected()
                ? 'indeterminate'
                : false
          }
          onCheckedChange={checked => table.toggleAllRowsSelected(checked === true)}
          onClick={event => event.stopPropagation()}
          aria-label={t('grid.selectAllRows', { defaultValue: 'Select all rows' })}
          className="h-4 w-4 rounded border-border cursor-pointer"
        />
      ),
      cell: ({ row }) => (
        <Checkbox
          checked={row.getIsSelected()}
          onCheckedChange={checked => row.toggleSelected(checked === true)}
          onClick={event => event.stopPropagation()}
          aria-label={t('grid.selectRow', { defaultValue: 'Select row' })}
          className="h-4 w-4 rounded border-border cursor-pointer"
        />
      ),
      size: 40,
    });

    const dataColumns = result.columns.map(col => {
      const isPrimaryKey = primaryKeySet.has(col.name);
      const columnForeignKeys = foreignKeyMap.get(col.name);
      const isForeignKey = Boolean(columnForeignKeys?.length);
      const fkTable = columnForeignKeys?.[0]?.referenced_table;
      const isVirtualFk = columnForeignKeys?.some(fk => fk.is_virtual) ?? false;

      return columnHelper.accessor(row => row[col.name], {
        id: col.name,
        header: ({ column }) => (
          <DataGridColumnHeader
            column={column}
            columnName={col.name}
            isPrimaryKey={isPrimaryKey}
            isForeignKey={isForeignKey}
            isVirtualFk={isVirtualFk}
            fkTable={fkTable}
            isIndexed={indexedColumns.has(col.name)}
            isUnique={uniqueColumns.has(col.name)}
            indexName={indexInfoMap.get(col.name)?.name}
            isCompositeIndex={indexInfoMap.get(col.name)?.isComposite}
          />
        ),
        cell: info => {
          const value = info.getValue();
          const isEditing =
            editingCellRef.current?.rowId === info.row.id &&
            editingCellRef.current.columnId === info.column.id;
          const foreignKeys = foreignKeyMap.get(info.column.id);
          const foreignKey = foreignKeys?.[0];
          const canPeek =
            Boolean(foreignKey) &&
            !isEditing &&
            value !== null &&
            Boolean(sessionId && namespace && tableName);
          const peekKey = canPeek && foreignKey ? buildPeekKey(foreignKey, value) : undefined;
          const peekState = peekKey ? peekCache.get(peekKey) : undefined;
          const relationLabel = foreignKey ? getRelationLabel(foreignKey) : '';
          const referencedNamespace = foreignKey ? resolveReferencedNamespace(foreignKey) : null;
          const hasMultipleRelations = Boolean(foreignKeys && foreignKeys.length > 1);

          return (
            <EditableDataCell
              value={value}
              columnId={info.column.id}
              rowId={info.row.id}
              row={info.row.original}
              isEditing={isEditing}
              editingValue={editingValueRef.current}
              editInputRef={editInputRef}
              onStartEdit={() =>
                startInlineEdit(info.row.original, info.row.id, info.column.id, value)
              }
              onCommitEdit={commitInlineEdit}
              onCancelEdit={cancelInlineEdit}
              onEditValueChange={setEditingValue}
              inlineEditAvailable={inlineEditAvailable}
              foreignKey={foreignKey}
              peekKey={peekKey}
              peekState={peekState}
              canPeek={canPeek}
              onEnsurePeekLoaded={() => foreignKey && ensurePeekLoaded(foreignKey, value)}
              relationLabel={relationLabel}
              referencedNamespace={referencedNamespace}
              hasMultipleRelations={hasMultipleRelations}
              onOpenRelatedTable={onOpenRelatedTable}
            />
          );
        },
        sortingFn: (rowA, rowB, columnId) => {
          const a = rowA.getValue(columnId) as Value;
          const b = rowB.getValue(columnId) as Value;
          if (a === null && b === null) return 0;
          if (a === null) return 1;
          if (b === null) return -1;
          if (typeof a === 'number' && typeof b === 'number') return a - b;
          return String(a).localeCompare(String(b));
        },
      });
    });

    const leadingColumns = actionColumn ? [selectColumn, actionColumn] : [selectColumn];
    return [...leadingColumns, ...dataColumns];
  }, [
    onRowClick,
    result,
    t,
    startInlineEdit,
    commitInlineEdit,
    cancelInlineEdit,
    setEditingValue,
    inlineEditAvailable,
    foreignKeyMap,
    primaryKeySet,
    buildPeekKey,
    peekCache,
    ensurePeekLoaded,
    getRelationLabel,
    resolveReferencedNamespace,
    onOpenRelatedTable,
    sessionId,
    namespace,
    tableName,
    editingCellRef,
    editingValueRef,
    editInputRef,
    indexedColumns,
    uniqueColumns,
    indexInfoMap,
  ]);

  // Configure table
  const table = useReactTable({
    data,
    columns,
    state: {
      sorting,
      rowSelection,
      ...(isInfiniteScrollMode ? {} : { pagination }),
      globalFilter,
      columnVisibility,
      columnFilters,
    },
    onSortingChange: handleSortingChange,
    onRowSelectionChange: setRowSelection,
    ...(isInfiniteScrollMode ? {} : { onPaginationChange: setPagination }),
    onGlobalFilterChange: setGlobalFilter,
    onColumnVisibilityChange: setColumnVisibility,
    onColumnFiltersChange: setColumnFilters,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    ...(isInfiniteScrollMode ? {} : { getPaginationRowModel: getPaginationRowModel() }),
    getFilteredRowModel: getFilteredRowModel(),
    enableRowSelection: true,
    globalFilterFn: 'includesString',
    enableColumnResizing: true,
    columnResizeMode: 'onChange',
    manualPagination: isInfiniteScrollMode,
    manualSorting: isServerSideSorting,
    manualFiltering: isServerSideMode,
  });

  const { rows } = table.getRowModel();

  // Delete hook
  const {
    isDeleting,
    deleteDialogOpen,
    setDeleteDialogOpen,
    deleteConfirmValue,
    setDeleteConfirmValue,
    handleDelete,
    performDelete,
    canDelete,
    deleteDisabled,
    deleteRequiresConfirm,
    previewRows,
  } = useDataGridDelete({
    table,
    sessionId,
    namespace,
    tableName,
    primaryKey,
    environment,
    readOnly,
    mutationsSupported,
    sandboxMode,
    onSandboxDelete,
    onRowsDeleted,
  });

  // Virtual scrolling
  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 32,
    overscan: 10,
  });

  // Infinite scroll: fetch more data when near bottom
  useEffect(() => {
    if (!isInfiniteScrollMode || !onFetchMore) return;

    const scrollEl = parentRef.current;
    if (!scrollEl) return;

    const handleScroll = () => {
      const { scrollTop, scrollHeight, clientHeight } = scrollEl;
      const distanceFromBottom = scrollHeight - scrollTop - clientHeight;

      if (distanceFromBottom < 500 && !infiniteScrollIsFetchingMore && !infiniteScrollIsComplete) {
        onFetchMore();
      }
    };

    scrollEl.addEventListener('scroll', handleScroll, { passive: true });
    return () => scrollEl.removeEventListener('scroll', handleScroll);
  }, [isInfiniteScrollMode, onFetchMore, infiniteScrollIsFetchingMore, infiniteScrollIsComplete]);

  // Scroll to top when data resets (sort/search change)
  const prevLoadedRows = useRef(infiniteScrollLoadedRows);
  useEffect(() => {
    if (
      isInfiniteScrollMode &&
      prevLoadedRows.current !== undefined &&
      prevLoadedRows.current > 0 &&
      (infiniteScrollLoadedRows === 0 || infiniteScrollLoadedRows === undefined)
    ) {
      parentRef.current?.scrollTo(0, 0);
    }
    prevLoadedRows.current = infiniteScrollLoadedRows;
  }, [isInfiniteScrollMode, infiniteScrollLoadedRows]);

  // Copy/export hooks
  const getSelectedRows = useCallback(() => table.getSelectedRowModel().rows, [table]);

  const { copyToClipboard, copied } = useDataGridCopy({
    rows,
    getSelectedRows,
    result,
    tableName,
  });

  const { exportToFile } = useDataGridExport({
    rows,
    getSelectedRows,
    result,
    tableName,
  });

  const { startStreamingExport } = useStreamingExport(sessionId);
  const [streamingDialogOpen, setStreamingDialogOpen] = useState(false);
  const canStreamExport = Boolean(sessionId && exportQuery);

  const handleStreamingExportConfirm = useCallback(
    async (config: ExportConfig) => {
      const exportId = await startStreamingExport(config);
      if (exportId) {
        setStreamingDialogOpen(false);
      }
    },
    [startStreamingExport]
  );

  // AI Explain Results
  const { isFeatureEnabled } = useLicense();
  const { getConfig, isReady: aiReady } = useAiPreferences();
  const [aiExplanation, setAiExplanation] = useState<string | null>(null);
  const [aiExplainLoading, setAiExplainLoading] = useState(false);
  const canExplainWithAi = isFeatureEnabled('ai') && Boolean(sessionId) && aiReady;

  const handleExplainWithAi = useCallback(async () => {
    if (!sessionId || !result || aiExplainLoading) return;
    setAiExplainLoading(true);
    try {
      const summary = `${result.rows.length} rows, ${result.columns.length} columns (${result.columns.map(c => c.name).join(', ')})`;
      const queryUsed = exportQuery || '';
      const response = await aiExplainResult(sessionId, queryUsed, summary, getConfig(), namespace);
      setAiExplanation(response.content);
    } catch {
      setAiExplanation(null);
    } finally {
      setAiExplainLoading(false);
    }
  }, [sessionId, result, aiExplainLoading, exportQuery, getConfig, namespace]);

  // Keyboard shortcuts
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName.toLowerCase();
      if (tag === 'input' || tag === 'textarea' || target?.isContentEditable) {
        return;
      }
      if ((e.metaKey || e.ctrlKey) && e.key === 'f') {
        if (document.activeElement?.closest('[data-datagrid]')) {
          e.preventDefault();
          searchInputRef.current?.focus();
        }
      }
      if ((e.metaKey || e.ctrlKey) && e.key === 'c') {
        if (document.activeElement?.closest('[data-datagrid]')) {
          e.preventDefault();
          copyToClipboard('csv');
        }
      }
      if ((e.metaKey || e.ctrlKey) && e.key === 'a') {
        if (document.activeElement?.closest('[data-datagrid]')) {
          e.preventDefault();
          table.toggleAllRowsSelected(true);
        }
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [copyToClipboard, table]);

  useEffect(() => {
    const handler = (event: Event) => {
      const detail = (event as CustomEvent<ExportDataDetail>).detail;
      const format = detail?.format ?? 'csv';
      exportToFile(format);
    };
    window.addEventListener(UI_EVENT_EXPORT_DATA, handler);
    return () => window.removeEventListener(UI_EVENT_EXPORT_DATA, handler);
  }, [exportToFile]);

  // Early return for empty state (but never when a search filter is active)
  if ((!result || result.columns.length === 0) && !globalFilter) {
    if (result && typeof result.affected_rows === 'number') {
      const time = Math.round(result.execution_time_ms ?? 0);
      const message =
        result.affected_rows > 0
          ? t('results.affectedRows', { count: result.affected_rows, time })
          : t('results.commandOk', { time });

      return (
        <div className="flex flex-col items-center justify-center h-40 text-muted-foreground text-sm gap-2">
          <CheckCircle2 size={22} className="text-muted-foreground/60" />
          <span>{message}</span>
        </div>
      );
    }
    return (
      <div className="flex items-center justify-center h-40 text-muted-foreground text-sm">
        {t('grid.noData')}
      </div>
    );
  }

  const selectedCount = Object.keys(rowSelection).length;
  const selectedRows = table.getSelectedRowModel().rows;

  return (
    <div className="flex flex-col gap-2 h-full min-h-0" data-datagrid>
      <div className="flex items-center justify-between px-1 shrink-0">
        <DataGridHeader
          selectedCount={selectedCount}
          totalRows={totalRows}
          result={result}
          canDelete={canDelete}
          deleteDisabled={deleteDisabled}
          isDeleting={isDeleting}
          onDelete={handleDelete}
          readOnly={readOnly}
          mutationsSupported={mutationsSupported}
        />

        <DataGridToolbar
          table={table}
          globalFilter={globalFilter}
          setGlobalFilter={setGlobalFilter}
          searchInputRef={searchInputRef}
          copyToClipboard={copyToClipboard}
          onStreamingExport={canStreamExport ? () => setStreamingDialogOpen(true) : undefined}
          copied={!!copied}
          showFilters={showFilters}
          setShowFilters={setShowFilters}
          onExplainWithAi={canExplainWithAi ? handleExplainWithAi : undefined}
          aiExplanation={aiExplanation}
          aiExplainLoading={aiExplainLoading}
          onDismissAiExplanation={() => setAiExplanation(null)}
        />
      </div>

      <div ref={parentRef} className="border border-border rounded-md overflow-auto flex-1 min-h-0">
        <table className="w-full text-sm border-collapse relative">
          <DataGridTableHeader table={table} showFilters={showFilters} />
          <DataGridTableBody
            rows={rows}
            rowVirtualizer={rowVirtualizer}
            rowMetadataMap={overlayResult.rowMetadata}
            sandboxMode={sandboxMode}
            columnsCount={columns.length}
          />
        </table>
      </div>

      {isInfiniteScrollMode ? (
        <DataGridStatusBar
          loadedRows={infiniteScrollLoadedRows ?? 0}
          totalRows={infiniteScrollTotalRows ?? 0}
          isFetchingMore={infiniteScrollIsFetchingMore ?? false}
          isComplete={infiniteScrollIsComplete ?? false}
        />
      ) : (
        <DataGridPagination table={table} pagination={pagination} />
      )}

      {canStreamExport && exportQuery && (
        <StreamingExportDialog
          open={streamingDialogOpen}
          onOpenChange={setStreamingDialogOpen}
          query={exportQuery}
          namespace={namespace}
          tableName={tableName}
          onConfirm={handleStreamingExportConfirm}
        />
      )}

      <DeleteConfirmDialog
        open={deleteDialogOpen}
        onOpenChange={setDeleteDialogOpen}
        selectedCount={selectedCount}
        previewRows={previewRows}
        totalSelectedRows={selectedRows.length}
        requiresConfirm={deleteRequiresConfirm}
        confirmLabel={confirmationLabel}
        confirmValue={deleteConfirmValue}
        onConfirmValueChange={setDeleteConfirmValue}
        onConfirm={async () => {
          await performDelete(true);
          setDeleteDialogOpen(false);
        }}
        isDeleting={isDeleting}
      />

      <DangerConfirmDialog
        open={updateConfirmOpen}
        onOpenChange={open => {
          setUpdateConfirmOpen(open);
          if (!open) {
            setPendingUpdate(null);
          }
        }}
        title={t('grid.updateConfirmTitle')}
        description={t('grid.updateConfirmDescription', {
          table: tableName || '',
          column: pendingUpdate?.columnId || '',
        })}
        warningInfo={
          pendingUpdate && primaryKey?.length
            ? primaryKey
                .map(pk => `${pk}=${formatValue(pendingUpdate.row[pk] ?? null)}`)
                .join(' | ')
            : undefined
        }
        confirmationLabel={environment === 'production' ? confirmationLabel : undefined}
        confirmLabel={t('grid.updateConfirmLabel')}
        loading={isUpdating}
        onConfirm={async () => {
          if (!pendingUpdate) return;
          await performInlineUpdate(pendingUpdate, true);
          setUpdateConfirmOpen(false);
          setPendingUpdate(null);
        }}
      />
    </div>
  );
}
