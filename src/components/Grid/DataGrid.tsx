import { useMemo, useState, useCallback, useRef, useEffect } from 'react';
import {
	useReactTable,
	getCoreRowModel,
	getSortedRowModel,
	getPaginationRowModel,
	getFilteredRowModel,
	flexRender,
	createColumnHelper,
	SortingState,
	RowSelectionState,
	PaginationState,
	ColumnDef,
	VisibilityState,
	ColumnFiltersState,
} from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
	QueryResult,
	Value,
	Namespace,
	deleteRow,
	RowData as TauriRowData,
	Environment,
	updateRow,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { ArrowUpDown, ArrowUp, ArrowDown, Trash2, CheckCircle2, Pencil } from 'lucide-react';
import { Button } from "@/components/ui/button";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { RowData, formatValue, convertToRowData } from "./utils/dataGridUtils";
import { useDataGridCopy } from "./hooks/useDataGridCopy";
import { useDataGridExport } from "./hooks/useDataGridExport";
import { DataGridToolbar } from "./DataGridToolbar";
import { DataGridPagination } from "./DataGridPagination";
import { DeleteConfirmDialog } from "./DeleteConfirmDialog";
import { GridColumnFilter } from "./GridColumnFilter";
import { DangerConfirmDialog } from "@/components/Guard/DangerConfirmDialog";

interface DataGridProps {
	result: QueryResult | null;
	height?: number;
	sessionId?: string;
	namespace?: Namespace;
	tableName?: string;
	primaryKey?: string[];
	environment?: Environment;
	readOnly?: boolean;
	mutationsSupported?: boolean;
	connectionName?: string;
	connectionDatabase?: string;
	onRowsDeleted?: () => void;
	onRowClick?: (row: RowData) => void;
	onRowsUpdated?: () => void;
}

export function DataGrid({
  result,
  // height = 400, // Removed unused prop
  sessionId,
  namespace,
  tableName,
  primaryKey,
  environment = 'development',
  readOnly = false,
  mutationsSupported = true,
  connectionName,
  connectionDatabase,
  onRowsDeleted,
  onRowClick,
  onRowsUpdated,
}: DataGridProps) {
  const { t } = useTranslation();
  const DEFAULT_RENDER_LIMIT = 2000;
  const RENDER_STEP = 2000;

  // Table state
  const [sorting, setSorting] = useState<SortingState>([]);
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({});
  const [pagination, setPagination] = useState<PaginationState>({
    pageIndex: 0,
    pageSize: 50,
  });
  const [globalFilter, setGlobalFilter] = useState('');
  const [columnVisibility, setColumnVisibility] = useState<VisibilityState>({});
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([]);
  const [showFilters, setShowFilters] = useState(false);
  const [renderLimit, setRenderLimit] = useState<number | null>(DEFAULT_RENDER_LIMIT);
  const [editingCell, setEditingCell] = useState<{ rowId: string; columnId: string } | null>(null);
  const [, setEditingValue] = useState('');
  const [, setEditingInitialValue] = useState('');
  const [, setEditingOriginalValue] = useState<Value | undefined>(undefined);
  const [, setEditingRow] = useState<RowData | null>(null);
  const [updateConfirmOpen, setUpdateConfirmOpen] = useState(false);
  const [pendingUpdate, setPendingUpdate] = useState<{
    row: RowData;
    columnId: string;
    value: Value;
    originalValue: Value;
  } | null>(null);
  const [isUpdating, setIsUpdating] = useState(false);

  // Delete state
  const [isDeleting, setIsDeleting] = useState(false);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState('');

  // Refs
  const searchInputRef = useRef<HTMLInputElement>(null);
  const parentRef = useRef<HTMLDivElement>(null);
  const editInputRef = useRef<HTMLInputElement>(null);
  const skipCommitRef = useRef(false);
  const editingCellRef = useRef<{ rowId: string; columnId: string } | null>(null);
  const editingRowRef = useRef<RowData | null>(null);
  const editingValueRef = useRef('');
  const editingInitialValueRef = useRef('');
  const editingOriginalValueRef = useRef<Value | undefined>(undefined);
  const confirmationLabel = (connectionDatabase || connectionName || 'PROD').trim() || 'PROD';

  const totalRows = result?.rows.length ?? 0;

  const resetEditingState = useCallback(() => {
    setEditingCell(null);
    setEditingRow(null);
    setEditingValue('');
    setEditingInitialValue('');
    setEditingOriginalValue(undefined);
    editingCellRef.current = null;
    editingRowRef.current = null;
    editingValueRef.current = '';
    editingInitialValueRef.current = '';
    editingOriginalValueRef.current = undefined;
  }, []);

  useEffect(() => {
    setRenderLimit(DEFAULT_RENDER_LIMIT);
  }, [result]);

  useEffect(() => {
    resetEditingState();
  }, [resetEditingState, result]);

  useEffect(() => {
    if (!editingCell) return;
    requestAnimationFrame(() => {
      editInputRef.current?.focus();
      editInputRef.current?.select();
    });
  }, [editingCell]);

  const effectiveLimit = renderLimit === null ? totalRows : renderLimit;
  const isLimited = totalRows > effectiveLimit;

  // Convert data
  const data = useMemo(() => {
    if (!result) return [];
    const limitedRows = renderLimit === null ? result.rows : result.rows.slice(0, renderLimit);
    return convertToRowData({ ...result, rows: limitedRows });
  }, [result, renderLimit]);

  const columnTypeMap = useMemo(() => {
    const map = new Map<string, string>();
    result?.columns.forEach(col => map.set(col.name, col.data_type));
    return map;
  }, [result]);

  const hasInlineEditContext = Boolean(sessionId && namespace && tableName);
  const hasPrimaryKey = Boolean(primaryKey && primaryKey.length > 0);
  const inlineEditAvailable = hasInlineEditContext && hasPrimaryKey;

  const getEditableValue = (value: Value) => {
    if (value === null) return 'NULL';
    if (typeof value === 'boolean') return value ? 'true' : 'false';
    if (typeof value === 'number') return String(value);
    if (typeof value === 'string') return value;
    if (typeof value === 'object') return JSON.stringify(value);
    return String(value);
  };

  const parseInputValue = (raw: string, dataType?: string): Value => {
    const trimmed = raw.trim();
    if (trimmed.toLowerCase() === 'null') return null;

    const normalizedType = dataType?.toLowerCase() ?? '';
    if (normalizedType.includes('bool')) {
      if (trimmed.toLowerCase() === 'true') return true;
      if (trimmed.toLowerCase() === 'false') return false;
      return raw;
    }

    const numericTypes = ['int', 'decimal', 'numeric', 'float', 'double', 'real', 'serial'];
    if (numericTypes.some(type => normalizedType.includes(type))) {
      if (trimmed === '') return '';
      const numericValue = Number(trimmed);
      return Number.isNaN(numericValue) ? raw : numericValue;
    }

    if (normalizedType.includes('json')) {
      if (trimmed === '') return '';
      try {
        return JSON.parse(trimmed);
      } catch {
        return raw;
      }
    }

    return raw;
  };

  const valuesEqual = (a: Value, b: Value) => {
    if (a === b) return true;
    if (typeof a === 'object' && typeof b === 'object' && a && b) {
      try {
        return JSON.stringify(a) === JSON.stringify(b);
      } catch {
        return false;
      }
    }
    return false;
  };

  const startInlineEdit = useCallback(
    (row: RowData, rowId: string, columnId: string, currentValue: Value) => {
      skipCommitRef.current = false;
      if (
        editingCellRef.current?.rowId === rowId &&
        editingCellRef.current.columnId === columnId
      ) {
        return;
      }
      if (!hasInlineEditContext) return;
      if (!hasPrimaryKey) {
        toast.error(t('grid.updateNoPrimaryKey'));
        return;
      }
      if (readOnly) {
        toast.error(t('environment.blocked'));
        return;
      }
      if (!mutationsSupported) {
        toast.error(t('grid.mutationsNotSupported'));
        return;
      }

      const displayValue = getEditableValue(currentValue);
      const cellRef = { rowId, columnId };
      setEditingCell(cellRef);
      setEditingRow(row);
      setEditingValue(displayValue);
      setEditingInitialValue(displayValue);
      setEditingOriginalValue(currentValue);
      editingCellRef.current = cellRef;
      editingRowRef.current = row;
      editingValueRef.current = displayValue;
      editingInitialValueRef.current = displayValue;
      editingOriginalValueRef.current = currentValue;
    },
    [hasInlineEditContext, hasPrimaryKey, readOnly, mutationsSupported, t]
  );

  const performInlineUpdate = useCallback(
    async (payload: { row: RowData; columnId: string; value: Value; originalValue: Value }) => {
      if (!sessionId || !namespace || !tableName || !primaryKey || primaryKey.length === 0) {
        toast.error(t('grid.updateNoPrimaryKey'));
        return;
      }
      if (readOnly) {
        toast.error(t('environment.blocked'));
        return;
      }
      if (!mutationsSupported) {
        toast.error(t('grid.mutationsNotSupported'));
        return;
      }

      const pkData: TauriRowData = { columns: {} };
      for (const key of primaryKey) {
        if (payload.row[key] === undefined) {
          toast.error(t('grid.updateNoPrimaryKey'));
          return;
        }
        pkData.columns[key] = payload.row[key];
      }

      setIsUpdating(true);
      try {
        const res = await updateRow(
          sessionId,
          namespace.database,
          namespace.schema,
          tableName,
          pkData,
          { columns: { [payload.columnId]: payload.value } }
        );
        if (res.success) {
          toast.success(t('grid.updateSuccess'));
          onRowsUpdated?.();
        } else {
          toast.error(t('grid.updateError'));
        }
      } catch {
        toast.error(t('grid.updateError'));
      } finally {
        setIsUpdating(false);
      }
    },
    [
      sessionId,
      namespace,
      tableName,
      primaryKey,
      readOnly,
      mutationsSupported,
      onRowsUpdated,
      t,
    ]
  );

  const commitInlineEdit = useCallback(async () => {
    if (skipCommitRef.current) {
      skipCommitRef.current = false;
      return;
    }
    const currentCell = editingCellRef.current;
    const currentRow = editingRowRef.current;
    const initialValue = editingInitialValueRef.current;
    const currentValue = editingValueRef.current;
    const originalValue = editingOriginalValueRef.current;

    if (!currentCell || !currentRow || originalValue === undefined) return;
    const currentColumnId = currentCell.columnId;

    resetEditingState();

    if (currentValue === initialValue) return;

    const parsedValue = parseInputValue(currentValue, columnTypeMap.get(currentColumnId));
    if (valuesEqual(parsedValue, originalValue)) return;

    const payload = {
      row: currentRow,
      columnId: currentColumnId,
      value: parsedValue,
      originalValue,
    };

    if (environment === 'development') {
      await performInlineUpdate(payload);
    } else {
      setPendingUpdate(payload);
      setUpdateConfirmOpen(true);
    }
  }, [columnTypeMap, environment, performInlineUpdate, resetEditingState]);

  const cancelInlineEdit = useCallback(() => {
    skipCommitRef.current = true;
    resetEditingState();
  }, [resetEditingState]);

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
        <input
          type="checkbox"
          checked={table.getIsAllRowsSelected()}
          onChange={table.getToggleAllRowsSelectedHandler()}
          onClick={event => event.stopPropagation()}
          className="h-4 w-4 rounded border-border cursor-pointer"
        />
      ),
      cell: ({ row }) => (
        <input
          type="checkbox"
          checked={row.getIsSelected()}
          onChange={row.getToggleSelectedHandler()}
          onClick={event => event.stopPropagation()}
          className="h-4 w-4 rounded border-border cursor-pointer"
        />
      ),
      size: 40,
    });

    const dataColumns = result.columns.map(col =>
      columnHelper.accessor(row => row[col.name], {
        id: col.name,
        header: ({ column }) => (
          <button
            className="flex items-center gap-1 hover:text-foreground transition-colors w-full text-left"
            onClick={() => column.toggleSorting()}
          >
            <span className="truncate">{col.name}</span>
            {column.getIsSorted() === 'asc' ? (
              <ArrowUp size={14} className="shrink-0 text-accent" />
            ) : column.getIsSorted() === 'desc' ? (
              <ArrowDown size={14} className="shrink-0 text-accent" />
            ) : (
              <ArrowUpDown size={14} className="shrink-0 opacity-30" />
            )}
          </button>
        ),
        cell: info => {
          const value = info.getValue();
          const formatted = formatValue(value);
          const isNull = value === null;
          const isEditing =
            editingCellRef.current?.rowId === info.row.id &&
            editingCellRef.current.columnId === info.column.id;
          return (
            <div
              className={cn(
                'block',
                !isEditing && 'truncate',
                !isEditing && inlineEditAvailable && 'cursor-text'
              )}
              onClick={() => startInlineEdit(info.row.original, info.row.id, info.column.id, value)}
              onDoubleClick={() =>
                startInlineEdit(info.row.original, info.row.id, info.column.id, value)
              }
            >
              {isEditing ? (
                <input
                  ref={editInputRef}
                  value={editingValueRef.current}
                  onChange={event => {
                    const nextValue = event.target.value;
                    setEditingValue(nextValue);
                    editingValueRef.current = nextValue;
                  }}
                  onBlur={() => void commitInlineEdit()}
                  onKeyDown={event => {
                    if (event.key === 'Enter') {
                      event.preventDefault();
                      void commitInlineEdit();
                    }
                    if (event.key === 'Escape') {
                      event.preventDefault();
                      cancelInlineEdit();
                    }
                  }}
                  className="w-full bg-background border border-accent/50 rounded px-1.5 py-0.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-accent/40"
                  aria-label={t('grid.editCell')}
                />
              ) : (
                <span className={cn('truncate block', isNull && 'text-muted-foreground italic')}>
                  {formatted}
                </span>
              )}
            </div>
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
      })
    );

    const leadingColumns = actionColumn ? [selectColumn, actionColumn] : [selectColumn];
    return [...leadingColumns, ...dataColumns];
  }, [
    onRowClick,
    result,
    t,
    startInlineEdit,
    commitInlineEdit,
    cancelInlineEdit,
    inlineEditAvailable,
  ]);

  // Configure table
  const table = useReactTable({
    data,
    columns,
    state: {
      sorting,
      rowSelection,
      pagination,
      globalFilter,
      columnVisibility,
      columnFilters,
    },
    onSortingChange: setSorting,
    onRowSelectionChange: setRowSelection,
    onPaginationChange: setPagination,
    onGlobalFilterChange: setGlobalFilter,
    onColumnVisibilityChange: setColumnVisibility,
    onColumnFiltersChange: setColumnFilters,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    enableRowSelection: true,
    globalFilterFn: 'includesString',
    enableColumnResizing: true,
    columnResizeMode: 'onChange',
  });

  const { rows } = table.getRowModel();

  // Virtual scrolling
  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 32,
    overscan: 10,
  });

  // Hooks
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

  const handleLoadMore = useCallback(() => {
    if (renderLimit === null) return;
    const nextLimit = Math.min(totalRows, renderLimit + RENDER_STEP);
    setRenderLimit(nextLimit);
  }, [renderLimit, totalRows]);

  const handleShowAll = useCallback(() => {
    setRenderLimit(null);
  }, []);

  // Delete functionality
  async function performDelete() {
    if (!sessionId || !namespace || !tableName || !primaryKey || primaryKey.length === 0) return;

    const selectedRows = table.getSelectedRowModel().rows;
    if (selectedRows.length === 0) return;

    if (readOnly) {
      toast.error(t('environment.blocked'));
      return;
    }
    if (!mutationsSupported) {
      toast.error(t('grid.mutationsNotSupported'));
      return;
    }

    setIsDeleting(true);
    let successCount = 0;
    let failCount = 0;

    for (const row of selectedRows) {
      const pkData: TauriRowData = { columns: {} };
      let missingPk = false;

      for (const key of primaryKey) {
        if (row.original[key] === undefined) {
          missingPk = true;
          break;
        }
        pkData.columns[key] = row.original[key];
      }

      if (missingPk) {
        failCount++;
        continue;
      }

      try {
        const res = await deleteRow(
          sessionId,
          namespace.database,
          namespace.schema,
          tableName,
          pkData
        );
        if (res.success) {
          successCount++;
        } else {
          failCount++;
        }
      } catch {
        failCount++;
      }
    }

    setIsDeleting(false);
    table.resetRowSelection();

    if (successCount > 0) {
      toast.success(t('grid.deleteSuccess', { count: successCount }));
      onRowsDeleted?.();
    }
    if (failCount > 0) {
      toast.error(t('grid.deleteError'));
    }
  }

  function handleDelete() {
    const selectedRows = table.getSelectedRowModel().rows;
    if (selectedRows.length === 0) return;

    if (readOnly) {
      toast.error(t('environment.blocked'));
      return;
    }
    if (!mutationsSupported) {
      toast.error(t('grid.mutationsNotSupported'));
      return;
    }

    setDeleteConfirmValue('');
    setDeleteDialogOpen(true);
  }

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

  // Early return for empty state
  if (!result || result.columns.length === 0) {
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

  // Computed values
  const selectedCount = Object.keys(rowSelection).length;
  const selectedRows = table.getSelectedRowModel().rows;
  const canDelete =
    sessionId && namespace && tableName && primaryKey && primaryKey.length > 0 && selectedCount > 0;
  const deleteDisabled = selectedCount === 0 || isDeleting || readOnly || !mutationsSupported;
  const deleteRequiresConfirm = environment === 'production';

  const previewRows = selectedRows.slice(0, 10).map((row, index) => {
    const values =
      primaryKey?.map(pk => ({
        key: pk,
        value: row.original[pk],
      })) || [];
    return {
      index: index + 1,
      values,
      hasMissing: values.some(entry => entry.value === undefined),
    };
  });

  return (
    <div className="flex flex-col gap-2 h-full min-h-0 overflow-hidden" data-datagrid>
      {/* Header */}
      <div className="flex items-center justify-between px-1 shrink-0">
        <div className="text-xs text-muted-foreground flex items-center gap-3">
          {selectedCount > 0 ? (
            <>
              <span>{t('grid.rowsSelected', { count: selectedCount })}</span>
              {isLimited && (
                <span>{t('grid.rowsShowing', { shown: data.length, total: totalRows })}</span>
              )}
            </>
          ) : (
            <div className="flex items-center gap-3">
              <span>{t('grid.rowsTotal', { count: totalRows })}</span>
              {isLimited && (
                <span>{t('grid.rowsShowing', { shown: data.length, total: totalRows })}</span>
              )}
              {result && typeof result.execution_time_ms === 'number' && (
                <div className="flex items-center gap-2 border-l border-border pl-3 ml-1">
                  <span title={t('query.time.execTooltip')}>
                    {t('query.time.exec')}:{' '}
                    <span className="font-mono text-foreground font-medium">
                      {result.execution_time_ms.toFixed(2)}ms
                    </span>
                  </span>
                  {result.total_time_ms !== undefined && (
                    <>
                      <span className="text-border/50">|</span>
                      <span title={t('query.time.totalTooltip')}>
                        {t('query.time.total')}:{' '}
                        <span className="font-mono text-foreground font-bold">
                          {result.total_time_ms.toFixed(2)}ms
                        </span>
                      </span>
                    </>
                  )}
                </div>
              )}
            </div>
          )}

          {isLimited && (
            <div className="flex items-center gap-2">
              <Button
                variant="ghost"
                size="sm"
                className="h-6 px-2 text-xs"
                onClick={handleLoadMore}
              >
                {t('grid.loadMore')}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                className="h-6 px-2 text-xs"
                onClick={handleShowAll}
              >
                {t('grid.showAll')}
              </Button>
            </div>
          )}

          {canDelete && (
            <Button
              variant="destructive"
              size="sm"
              className="h-6 px-2 text-xs"
              onClick={handleDelete}
              disabled={deleteDisabled}
              title={
                readOnly
                  ? t('environment.blocked')
                  : !mutationsSupported
                    ? t('grid.mutationsNotSupported')
                    : undefined
              }
            >
              <Trash2 size={12} className="mr-1" />
              {isDeleting ? t('grid.deleting') : t('grid.delete')}
            </Button>
          )}
        </div>

        <DataGridToolbar
          table={table}
          globalFilter={globalFilter}
          setGlobalFilter={setGlobalFilter}
          searchInputRef={searchInputRef}
          copyToClipboard={copyToClipboard}
          exportToFile={exportToFile}
          copied={!!copied}
          showFilters={showFilters}
          setShowFilters={setShowFilters}
        />
      </div>

      {/* Table */}
      <div ref={parentRef} className="border border-border rounded-md overflow-auto flex-1 min-h-0">
        <table className="w-full text-sm border-collapse relative">
          <thead className="sticky top-0 z-10 bg-muted/80 backdrop-blur-sm shadow-sm">
            {table.getHeaderGroups().map(headerGroup => (
              <tr key={headerGroup.id}>
                {headerGroup.headers.map(header => (
                  <th
                    key={header.id}
                    className="px-3 py-2 text-left font-medium text-muted-foreground border-b border-border relative group"
                    style={{ width: header.getSize() }}
                  >
                    {header.isPlaceholder
                      ? null
                      : flexRender(header.column.columnDef.header, header.getContext())}
                    {header.column.getCanResize() && (
                      <div
                        onMouseDown={header.getResizeHandler()}
                        onTouchStart={header.getResizeHandler()}
                        onDoubleClick={() => header.column.resetSize()}
                        className={cn(
                          'absolute right-0 top-0 h-full w-1 cursor-col-resize select-none touch-none',
                          'opacity-0 group-hover:opacity-100 hover:bg-accent transition-opacity',
                          header.column.getIsResizing() && 'bg-accent opacity-100'
                        )}
                      />
                    )}

                    {showFilters && header.column.getCanFilter() && (
                      <div className="mt-2" onClick={e => e.stopPropagation()}>
                        <GridColumnFilter column={header.column} />
                      </div>
                    )}
                  </th>
                ))}
              </tr>
            ))}
          </thead>
          <tbody>
            {rowVirtualizer.getVirtualItems().length > 0 ? (
              <>
                <tr
                  style={{
                    height: `${rowVirtualizer.getVirtualItems()[0]?.start ?? 0}px`,
                  }}
                />
                {rowVirtualizer.getVirtualItems().map(virtualRow => {
                  const row = rows[virtualRow.index];
                  return (
                    <tr
                      key={row.id}
                      className={cn(
                        'border-b border-border hover:bg-muted/50 transition-colors',
                        row.getIsSelected() && 'bg-accent/10'
                      )}
                    >
                      {row.getVisibleCells().map(cell => (
                        <td
                          key={cell.id}
                          className="px-3 py-1.5 max-w-xs"
                          style={{ width: cell.column.getSize() }}
                        >
                          {flexRender(cell.column.columnDef.cell, cell.getContext())}
                        </td>
                      ))}
                    </tr>
                  );
                })}
                <tr
                  style={{
                    height: `${rowVirtualizer.getTotalSize() - (rowVirtualizer.getVirtualItems()[rowVirtualizer.getVirtualItems().length - 1]?.end ?? 0)}px`,
                  }}
                />
              </>
            ) : (
              <tr>
                <td colSpan={columns.length} className="text-center py-8 text-muted-foreground">
                  {t('grid.noResults')}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <DataGridPagination table={table} pagination={pagination} />

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
          await performDelete();
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
          await performInlineUpdate(pendingUpdate);
          setUpdateConfirmOpen(false);
          setPendingUpdate(null);
        }}
      />
    </div>
  );
}
