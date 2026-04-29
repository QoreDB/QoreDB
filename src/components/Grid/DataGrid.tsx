// SPDX-License-Identifier: Apache-2.0

import {
  type ColumnDef,
  type ColumnFiltersState,
  type ColumnPinningState,
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
import {
  ShareExportDialog,
  type ShareExportDialogRequest,
} from '@/components/Share/ShareExportDialog';
import { SaveSnapshotDialog } from '@/components/Snapshot/SaveSnapshotDialog';
import { Checkbox } from '@/components/ui/checkbox';
import { useShareLinks } from '@/hooks/useShareLinks';
import { useStreamingExport } from '@/hooks/useStreamingExport';
import { aiExplainResult } from '@/lib/ai';
import { BULK_EDIT_CORE_LIMIT } from '@/lib/bulkEdit';
import type { Driver } from '@/lib/drivers';
import type { ExportConfig } from '@/lib/export';
import { applyOverlay, emptyOverlayResult, type OverlayResult } from '@/lib/sandboxOverlay';
import type { SandboxChange, SandboxDeleteDisplay } from '@/lib/sandboxTypes';
import type {
  ColumnFilter,
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
import { BulkEditDialog } from './BulkEditDialog';
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
  infiniteScrollTotalRows?: number;
  infiniteScrollLoadedRows?: number;
  infiniteScrollIsFetchingMore?: boolean;
  infiniteScrollIsComplete?: boolean;
  onFetchMore?: () => void;
  serverSortColumn?: string;
  serverSortDirection?: SortDirection;
  onServerSortChange?: (column?: string, direction?: SortDirection) => void;
  serverSearchTerm?: string;
  onServerSearchChange?: (term: string) => void;
  onServerColumnFiltersChange?: (filters: ColumnFilter[]) => void;
  exportQuery?: string;
  footerMode?: 'auto' | 'pagination' | 'infinite' | 'none';
  /** SQL dialect for embedded previews (Bulk Edit, etc.). Defaults to Postgres. */
  driver?: Driver;
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
  onServerColumnFiltersChange,
  exportQuery,
  footerMode = 'auto',
  driver,
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
  const resolvedFooterMode =
    footerMode === 'auto' ? (isInfiniteScrollMode ? 'infinite' : 'pagination') : footerMode;
  const noopServerSearchChange = useCallback((_term: string) => {}, []);
  const globalFilter = isServerSideMode ? (serverSearchTerm ?? '') : internalGlobalFilter;
  const setGlobalFilter = isServerSideMode
    ? (onServerSearchChange ?? noopServerSearchChange)
    : setInternalGlobalFilter;

  const [columnVisibility, setColumnVisibility] = useState<VisibilityState>({});
  const [columnPinning, setColumnPinning] = useState<ColumnPinningState>({
    left: [],
    right: [],
  });
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([]);
  const [showFilters, setShowFilters] = useState(false);
  const [bulkEditDialogOpen, setBulkEditDialogOpen] = useState(false);
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

  const handleColumnFiltersChange = useCallback(
    (updater: ColumnFiltersState | ((old: ColumnFiltersState) => ColumnFiltersState)) => {
      const nextFilters = typeof updater === 'function' ? updater(columnFilters) : updater;
      setColumnFilters(nextFilters);

      if (!isServerSideMode || !onServerColumnFiltersChange) return;

      const backendFilters: ColumnFilter[] = nextFilters
        .filter(f => f.value !== '' && f.value != null)
        .map(f => {
          const raw = f.value;
          if (raw && typeof raw === 'object' && 'operator' in raw) {
            const cell = raw as {
              operator: ColumnFilter['operator'];
              value: string;
              regex_flags?: string;
              text_language?: string;
            };
            const backend: ColumnFilter = {
              column: f.id,
              operator: cell.operator,
              value: cell.operator === 'like' ? `%${cell.value}%` : cell.value,
            };
            if (cell.regex_flags || cell.text_language) {
              backend.options = {
                ...(cell.regex_flags ? { regex_flags: cell.regex_flags } : {}),
                ...(cell.text_language ? { text_language: cell.text_language } : {}),
              };
            }
            return backend;
          }
          return {
            column: f.id,
            operator: 'like' as const,
            value: `%${String(raw)}%`,
          };
        });

      onServerColumnFiltersChange(backendFilters);
    },
    [columnFilters, isServerSideMode, onServerColumnFiltersChange]
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
    return convertToRowData(effectiveResult);
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

  // Stable row identity: prefer primary key when fully defined so that sort,
  // filter, and streaming batches don't unmount/remount existing rows. Falls
  // back to row index for tables without a PK or rows with NULL PK values
  // (e.g. sandbox-inserted rows pending an autoincrement).
  const getRowId = useMemo(() => {
    if (!primaryKey || primaryKey.length === 0) return undefined;
    return (row: RowData, index: number) => {
      let composite = '';
      for (const key of primaryKey) {
        const v = row[key];
        if (v === null || v === undefined) return `__idx_${index}`;
        composite += `${composite ? '::' : ''}${String(v)}`;
      }
      return composite;
    };
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

  useEffect(() => {
    cancelInlineEdit();
  }, [cancelInlineEdit]);

  // ── Stable refs for volatile values ──────────────────────────────────
  // These let the cell render function read current values at render time
  // without forcing a rebuild of all column definitions on every change.
  const peekCacheRef = useRef(peekCache);
  peekCacheRef.current = peekCache;
  const buildPeekKeyRef = useRef(buildPeekKey);
  buildPeekKeyRef.current = buildPeekKey;
  const ensurePeekLoadedRef = useRef(ensurePeekLoaded);
  ensurePeekLoadedRef.current = ensurePeekLoaded;
  const getRelationLabelRef = useRef(getRelationLabel);
  getRelationLabelRef.current = getRelationLabel;
  const resolveReferencedNamespaceRef = useRef(resolveReferencedNamespace);
  resolveReferencedNamespaceRef.current = resolveReferencedNamespace;
  const startInlineEditRef = useRef(startInlineEdit);
  startInlineEditRef.current = startInlineEdit;
  const commitInlineEditRef = useRef(commitInlineEdit);
  commitInlineEditRef.current = commitInlineEdit;
  const cancelInlineEditRef = useRef(cancelInlineEdit);
  cancelInlineEditRef.current = cancelInlineEdit;
  const setEditingValueRef = useRef(setEditingValue);
  setEditingValueRef.current = setEditingValue;
  const inlineEditAvailableRef = useRef(inlineEditAvailable);
  inlineEditAvailableRef.current = inlineEditAvailable;
  const onOpenRelatedTableRef = useRef(onOpenRelatedTable);
  onOpenRelatedTableRef.current = onOpenRelatedTable;
  const sessionIdRef = useRef(sessionId);
  sessionIdRef.current = sessionId;
  const namespaceRef = useRef(namespace);
  namespaceRef.current = namespace;
  const tableNameRef = useRef(tableName);
  tableNameRef.current = tableName;

  // biome-ignore lint/correctness/useExhaustiveDependencies: deliberately depend on `result?.columns` (not full `result`) so streaming row batches don't rebuild every ColumnDef. *Ref objects are stable; their `.current` is read at render time inside cells.
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
            Boolean(sessionIdRef.current && namespaceRef.current && tableNameRef.current);
          const peekKey =
            canPeek && foreignKey ? buildPeekKeyRef.current(foreignKey, value) : undefined;
          const peekState = peekKey ? peekCacheRef.current.get(peekKey) : undefined;
          const relationLabel = foreignKey ? getRelationLabelRef.current(foreignKey) : '';
          const referencedNamespace = foreignKey
            ? resolveReferencedNamespaceRef.current(foreignKey)
            : null;
          const hasMultipleRelations = Boolean(foreignKeys && foreignKeys.length > 1);

          return (
            <EditableDataCell
              value={value}
              columnId={info.column.id}
              rowId={info.row.id}
              row={info.row.original}
              dataType={col.data_type}
              isEditing={isEditing}
              editingValue={editingValueRef.current}
              editInputRef={editInputRef}
              onStartEdit={() =>
                startInlineEditRef.current(info.row.original, info.row.id, info.column.id, value)
              }
              onCommitEdit={() => void commitInlineEditRef.current()}
              onCancelEdit={() => cancelInlineEditRef.current()}
              onEditValueChange={v => setEditingValueRef.current(v)}
              inlineEditAvailable={inlineEditAvailableRef.current}
              foreignKey={foreignKey}
              peekKey={peekKey}
              peekState={peekState}
              canPeek={canPeek}
              onEnsurePeekLoaded={() =>
                foreignKey && ensurePeekLoadedRef.current(foreignKey, value)
              }
              relationLabel={relationLabel}
              referencedNamespace={referencedNamespace}
              hasMultipleRelations={hasMultipleRelations}
              onOpenRelatedTable={onOpenRelatedTableRef.current}
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
    // Deps deliberately exclude `result` (rows change on every streaming batch)
    // and the *Ref objects (stable identities). Only column metadata and
    // schema-derived sets should trigger column rebuild.
  }, [
    onRowClick,
    result?.columns,
    t,
    foreignKeyMap,
    primaryKeySet,
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
      columnPinning,
      columnFilters,
    },
    onSortingChange: handleSortingChange,
    onRowSelectionChange: setRowSelection,
    ...(isInfiniteScrollMode ? {} : { onPaginationChange: setPagination }),
    onGlobalFilterChange: setGlobalFilter,
    onColumnVisibilityChange: setColumnVisibility,
    onColumnPinningChange: setColumnPinning,
    onColumnFiltersChange: handleColumnFiltersChange,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    ...(isInfiniteScrollMode ? {} : { getPaginationRowModel: getPaginationRowModel() }),
    getFilteredRowModel: getFilteredRowModel(),
    ...(getRowId ? { getRowId } : {}),
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
  const [shareDialogOpen, setShareDialogOpen] = useState(false);
  const [snapshotDialogOpen, setSnapshotDialogOpen] = useState(false);
  const canStreamExport = Boolean(sessionId && exportQuery);
  const { startShareExport } = useShareLinks(sessionId);

  const handleStreamingExportConfirm = useCallback(
    async (config: ExportConfig) => {
      const exportId = await startStreamingExport(config);
      if (exportId) {
        setStreamingDialogOpen(false);
      }
    },
    [startStreamingExport]
  );

  const handleShareExportConfirm = useCallback(
    async (config: ShareExportDialogRequest) => {
      if (!exportQuery) return;

      const shareUrl = await startShareExport({
        query: exportQuery,
        namespace,
        file_name: config.file_name,
        format: config.format,
        include_headers: config.include_headers,
        table_name: config.table_name,
        batch_size: config.batch_size,
        limit: config.limit,
      });

      if (shareUrl) {
        setShareDialogOpen(false);
      }
    },
    [exportQuery, namespace, startShareExport]
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

  const hasBulkEditUnlimited = isFeatureEnabled('bulk_edit_unlimited');
  const canBulkEdit = Boolean(
    sessionId &&
      namespace &&
      tableName &&
      primaryKey &&
      primaryKey.length > 0 &&
      tableSchema &&
      selectedCount >= 2
  );
  const bulkEditRequiresPro = !hasBulkEditUnlimited && selectedCount > BULK_EDIT_CORE_LIMIT;
  const bulkEditDisabled =
    selectedCount < 2 || readOnly || !mutationsSupported || bulkEditRequiresPro;

  return (
    <div
      className="isolate flex h-full min-h-0 min-w-0 flex-col gap-2 contain-[paint]"
      data-datagrid
    >
      <div className="flex min-w-0 shrink-0 items-center justify-between gap-3 px-1">
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
          canBulkEdit={canBulkEdit}
          bulkEditDisabled={bulkEditDisabled}
          bulkEditRequiresPro={bulkEditRequiresPro}
          onBulkEdit={() => setBulkEditDialogOpen(true)}
        />

        <DataGridToolbar
          table={table}
          globalFilter={globalFilter}
          setGlobalFilter={setGlobalFilter}
          searchInputRef={searchInputRef}
          copyToClipboard={copyToClipboard}
          onStreamingExport={canStreamExport ? () => setStreamingDialogOpen(true) : undefined}
          onShareExport={canStreamExport ? () => setShareDialogOpen(true) : undefined}
          copied={!!copied}
          showFilters={showFilters}
          setShowFilters={setShowFilters}
          onSaveSnapshot={result ? () => setSnapshotDialogOpen(true) : undefined}
          onExplainWithAi={canExplainWithAi ? handleExplainWithAi : undefined}
          aiExplanation={aiExplanation}
          aiExplainLoading={aiExplainLoading}
          onDismissAiExplanation={() => setAiExplanation(null)}
        />
      </div>

      <div
        ref={parentRef}
        className="relative min-h-0 min-w-0 flex-1 overflow-x-auto overflow-y-auto rounded-md border border-border [contain:layout_paint]"
      >
        <table
          className="relative min-w-full border-collapse text-sm"
          style={{
            tableLayout: 'fixed',
            width: `max(100%, ${table.getTotalSize()}px)`,
          }}
        >
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

      {resolvedFooterMode === 'infinite' ? (
        <DataGridStatusBar
          loadedRows={infiniteScrollLoadedRows ?? 0}
          totalRows={infiniteScrollTotalRows ?? 0}
          isFetchingMore={infiniteScrollIsFetchingMore ?? false}
          isComplete={infiniteScrollIsComplete ?? false}
        />
      ) : resolvedFooterMode === 'pagination' ? (
        <DataGridPagination table={table} pagination={pagination} />
      ) : null}

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

      {canStreamExport && exportQuery && (
        <ShareExportDialog
          open={shareDialogOpen}
          onOpenChange={setShareDialogOpen}
          defaultFileName={tableName || 'query-results'}
          defaultTableName={tableName}
          onConfirm={handleShareExportConfirm}
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

      <BulkEditDialog
        open={bulkEditDialogOpen}
        onOpenChange={setBulkEditDialogOpen}
        selectedRows={selectedRows.map(r => r.original)}
        tableSchema={tableSchema ?? null}
        primaryKey={primaryKey}
        namespace={namespace}
        tableName={tableName}
        sessionId={sessionId}
        dialect={driver}
        sandboxMode={sandboxMode}
        onSandboxUpdate={onSandboxUpdate}
        onApplied={() => {
          table.resetRowSelection();
          onRowsUpdated?.();
        }}
      />

      {result && (
        <SaveSnapshotDialog
          open={snapshotDialogOpen}
          onOpenChange={setSnapshotDialogOpen}
          result={result}
          source={exportQuery || tableName || 'query'}
          sourceType={tableName ? 'table' : 'query'}
          connectionName={connectionName}
          driver={undefined}
          namespace={namespace}
          defaultName={
            tableName
              ? `${tableName} - ${new Date().toLocaleDateString()}`
              : `Query - ${new Date().toLocaleDateString()}`
          }
        />
      )}

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
