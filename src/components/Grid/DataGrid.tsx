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
  TableSchema,
  ForeignKey,
  peekForeignKey,
  RelationFilter,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { ArrowUpDown, ArrowUp, ArrowDown, Trash2, CheckCircle2, Pencil, Loader2, Link2 } from 'lucide-react';
import { Button } from "@/components/ui/button";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { TooltipRoot, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";

import { RowData, formatValue, convertToRowData } from "./utils/dataGridUtils";
import { useDataGridCopy } from "./hooks/useDataGridCopy";
import { useDataGridExport } from "./hooks/useDataGridExport";
import { DataGridToolbar } from "./DataGridToolbar";
import { DataGridPagination } from "./DataGridPagination";
import { DeleteConfirmDialog } from "./DeleteConfirmDialog";
import { GridColumnFilter } from "./GridColumnFilter";
import { DangerConfirmDialog } from "@/components/Guard/DangerConfirmDialog";
import { SandboxChange, SandboxDeleteDisplay, SandboxRowMetadata } from "@/lib/sandboxTypes";
import { applyOverlay, OverlayResult, emptyOverlayResult } from "@/lib/sandboxOverlay";

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
	onRowsDeleted?: () => void;
	onRowClick?: (row: RowData) => void;
	onRowsUpdated?: () => void;
  onOpenRelatedTable?: (
    namespace: Namespace,
    tableName: string,
    relationFilter?: RelationFilter
  ) => void;
  // Sandbox props
  sandboxMode?: boolean;
  pendingChanges?: SandboxChange[];
  sandboxDeleteDisplay?: SandboxDeleteDisplay;
  onSandboxInsert?: (newValues: Record<string, Value>) => void;
  onSandboxUpdate?: (primaryKey: Record<string, Value>, oldValues: Record<string, Value>, newValues: Record<string, Value>) => void;
  onSandboxDelete?: (primaryKey: Record<string, Value>, oldValues: Record<string, Value>) => void;
}

interface PeekState {
  status: 'idle' | 'loading' | 'ready' | 'error';
  result?: QueryResult;
  error?: string;
}

const MAX_PEEK_ROWS = 3;
const MAX_PEEK_COLUMNS = 6;
const PEEK_QUERY_LIMIT = 6;

function serializePeekValue(value: Value): string {
  if (value === null) return 'null';
  if (typeof value === 'object') {
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  }
  return String(value);
}

export function DataGrid({
  result,
  // height = 400, // Removed unused prop
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
  onRowsDeleted,
  onRowClick,
  onRowsUpdated,
  onOpenRelatedTable,
  // Sandbox props
  sandboxMode = false,
  pendingChanges = [],
  sandboxDeleteDisplay = 'strikethrough',
  onSandboxInsert,
  onSandboxUpdate,
  onSandboxDelete,
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

  // Foreign key peek state
  const [peekCache, setPeekCache] = useState<Map<string, PeekState>>(new Map());
  const peekRequests = useRef(new Set<string>());

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

  // Apply sandbox overlay to results
  const overlayResult: OverlayResult = useMemo(() => {
    if (!result || !sandboxMode || pendingChanges.length === 0 || !namespace || !tableName) {
      return result ? emptyOverlayResult(result) : { result: { columns: [], rows: [], affected_rows: undefined, execution_time_ms: 0 }, rowMetadata: new Map(), stats: { insertedRows: 0, modifiedRows: 0, deletedRows: 0, hiddenRows: 0 } };
    }
    return applyOverlay(result, pendingChanges, tableSchema ?? null, namespace, tableName, {
      deleteDisplay: sandboxDeleteDisplay,
      primaryKey,
    });
  }, [result, sandboxMode, pendingChanges, namespace, tableName, tableSchema, sandboxDeleteDisplay, primaryKey]);

  // Convert data (use overlayed result when in sandbox mode)
  const data = useMemo(() => {
    const effectiveResult = sandboxMode ? overlayResult.result : result;
    if (!effectiveResult) return [];
    const limitedRows = renderLimit === null ? effectiveResult.rows : effectiveResult.rows.slice(0, renderLimit);
    return convertToRowData({ ...effectiveResult, rows: limitedRows });
  }, [result, overlayResult.result, sandboxMode, renderLimit]);

  const columnTypeMap = useMemo(() => {
    const map = new Map<string, string>();
    result?.columns.forEach(col => map.set(col.name, col.data_type));
    return map;
  }, [result]);

  const foreignKeyMap = useMemo(() => {
    const map = new Map<string, ForeignKey[]>();
    if (!tableSchema?.foreign_keys?.length) return map;
    tableSchema.foreign_keys.forEach(fk => {
      if (!fk?.column) return;
      const entries = map.get(fk.column) ?? [];
      entries.push(fk);
      map.set(fk.column, entries);
    });
    return map;
  }, [tableSchema]);

  const hasInlineEditContext = Boolean(sessionId && namespace && tableName);
  const hasPrimaryKey = Boolean(primaryKey && primaryKey.length > 0);
  const inlineEditAvailable = hasInlineEditContext && hasPrimaryKey;

  const updatePeekCache = useCallback((key: string, next: PeekState) => {
    setPeekCache(prev => {
      const updated = new Map(prev);
      updated.set(key, next);
      return updated;
    });
  }, []);

  const resolveReferencedNamespace = useCallback(
    (foreignKey: ForeignKey): Namespace | null => {
      if (!namespace) return null;
      const database = foreignKey.referenced_database ?? namespace.database;
      const schema = foreignKey.referenced_schema ?? namespace.schema;
      return { database, schema };
    },
    [namespace]
  );

  const getRelationLabel = useCallback(
    (foreignKey: ForeignKey): string => {
      if (foreignKey.referenced_database) {
        return `${foreignKey.referenced_database}.${foreignKey.referenced_table}`;
      }
      if (foreignKey.referenced_schema) {
        return `${foreignKey.referenced_schema}.${foreignKey.referenced_table}`;
      }
      return foreignKey.referenced_table;
    },
    []
  );

  const buildPeekKey = useCallback(
    (foreignKey: ForeignKey, value: Value): string => {
      const nsKey = namespace ? `${namespace.database}:${namespace.schema ?? ''}` : 'unknown';
      const valueKey = serializePeekValue(value);
      return `${nsKey}:${foreignKey.referenced_table}:${foreignKey.referenced_column}:${valueKey}`;
    },
    [namespace]
  );

  const ensurePeekLoaded = useCallback(
    async (foreignKey: ForeignKey, value: Value) => {
      if (!sessionId || !namespace) return;
      const key = buildPeekKey(foreignKey, value);
      const cached = peekCache.get(key);
      if (cached?.status === 'loading' || cached?.status === 'ready') return;
      if (peekRequests.current.has(key)) return;
      peekRequests.current.add(key);
      updatePeekCache(key, { status: 'loading' });

      try {
        const response = await peekForeignKey(sessionId, namespace, foreignKey, value, PEEK_QUERY_LIMIT);
        if (response.success && response.result) {
          updatePeekCache(key, { status: 'ready', result: response.result });
        } else {
          updatePeekCache(key, {
            status: 'error',
            error: response.error || t('grid.peekFailed', { defaultValue: 'Preview unavailable' }),
          });
        }
      } catch (error) {
        updatePeekCache(key, {
          status: 'error',
          error:
            error instanceof Error
              ? error.message
              : t('grid.peekFailed', { defaultValue: 'Preview unavailable' }),
        });
      } finally {
        peekRequests.current.delete(key);
      }
    },
    [buildPeekKey, namespace, sessionId, t, updatePeekCache, peekCache]
  );

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
      if (!namespace || !tableName || !primaryKey || primaryKey.length === 0) {
        toast.error(t('grid.updateNoPrimaryKey'));
        return;
      }
      if (readOnly) {
        toast.error(t('environment.blocked'));
        return;
      }
      if (!mutationsSupported && !sandboxMode) {
        toast.error(t('grid.mutationsNotSupported'));
        return;
      }

      const pkData: Record<string, Value> = {};
      for (const key of primaryKey) {
        if (payload.row[key] === undefined) {
          toast.error(t('grid.updateNoPrimaryKey'));
          return;
        }
        pkData[key] = payload.row[key];
      }

      // Sandbox mode: add change locally
      if (sandboxMode && onSandboxUpdate) {
        const oldValues: Record<string, Value> = { [payload.columnId]: payload.originalValue };
        const newValues: Record<string, Value> = { [payload.columnId]: payload.value };
        onSandboxUpdate(pkData, oldValues, newValues);
        return;
      }

      // Real update
      if (!sessionId) {
        toast.error(t('grid.updateNoPrimaryKey'));
        return;
      }

      setIsUpdating(true);
      try {
        const res = await updateRow(
          sessionId,
          namespace.database,
          namespace.schema,
          tableName,
          { columns: pkData },
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
      sandboxMode,
      onSandboxUpdate,
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
          const foreignKeys = foreignKeyMap.get(info.column.id);
          const foreignKey = foreignKeys?.[0];
          const canPeek =
            Boolean(foreignKey) &&
            !isEditing &&
            value !== null &&
            Boolean(sessionId && namespace && tableName);
          const peekKey = canPeek && foreignKey ? buildPeekKey(foreignKey, value) : null;
          const peekState = peekKey ? peekCache.get(peekKey) : undefined;
          const relationLabel = foreignKey ? getRelationLabel(foreignKey) : '';
          const referencedNamespace = foreignKey ? resolveReferencedNamespace(foreignKey) : null;
          const hasMultipleRelations = Boolean(foreignKeys && foreignKeys.length > 1);

          const cellNode = (
            <div
              className={cn(
                'block',
                !isEditing && 'truncate',
                !isEditing && inlineEditAvailable && 'cursor-text',
                canPeek && 'group'
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
                <span
                  className={cn(
                    'truncate block',
                    isNull && 'text-muted-foreground italic',
                    canPeek && 'group-hover:text-foreground'
                  )}
                >
                  {formatted}
                </span>
              )}
            </div>
          );

          if (!canPeek || !foreignKey || !peekKey) {
            return cellNode;
          }

          const previewColumns = peekState?.result?.columns?.slice(0, MAX_PEEK_COLUMNS) ?? [];
          const previewRows = peekState?.result?.rows?.slice(0, MAX_PEEK_ROWS) ?? [];
          const extraColumns =
            peekState?.result?.columns && peekState.result.columns.length > MAX_PEEK_COLUMNS
              ? peekState.result.columns.length - MAX_PEEK_COLUMNS
              : 0;
          const extraRows =
            peekState?.result?.rows && peekState.result.rows.length > MAX_PEEK_ROWS
              ? peekState.result.rows.length - MAX_PEEK_ROWS
              : 0;

          return (
            <TooltipRoot
              delayDuration={400}
              disableHoverableContent={false}
              onOpenChange={open => {
                if (open) {
                  void ensurePeekLoaded(foreignKey, value);
                }
              }}
            >
              <TooltipTrigger asChild>{cellNode}</TooltipTrigger>
              <TooltipContent side="right" align="start" className="w-80 max-h-80 overflow-auto p-3 text-xs">
                <div className="flex items-start justify-between gap-2">
                  <div>
                    <div className="text-xs uppercase tracking-wide text-muted-foreground">
                      {t('grid.peekTitle', { defaultValue: 'Relation' })}
                    </div>
                    <div className="text-sm font-medium text-foreground">{relationLabel}</div>
                    {hasMultipleRelations && (
                      <div className="text-xs text-muted-foreground">
                        {t('grid.peekMultiple', { defaultValue: 'Multiple relations detected' })}
                      </div>
                    )}
                    {foreignKey.constraint_name && (
                      <div className="text-xs text-muted-foreground">
                        {foreignKey.constraint_name}
                      </div>
                    )}
                  </div>
                  {onOpenRelatedTable && referencedNamespace && (
                    <Button
                      variant="link"
                      size="sm"
                      className="h-auto px-0 text-xs"
                      onClick={event => {
                        event.preventDefault();
                        event.stopPropagation();
                        onOpenRelatedTable(referencedNamespace, foreignKey.referenced_table, {
                          foreignKey,
                          value,
                        });
                      }}
                    >
                      <Link2 size={12} />
                      {t('grid.openRelatedTable', { defaultValue: 'Open table' })}
                    </Button>
                  )}
                </div>
                <div className="mt-3 border-t border-border pt-3">
                  {peekState?.status === 'error' ? (
                    <div className="text-xs text-error">
                      {peekState.error || t('grid.peekFailed', { defaultValue: 'Preview unavailable' })}
                    </div>
                  ) : !peekState || peekState.status === 'loading' ? (
                    <div className="flex items-center gap-2 text-muted-foreground text-xs">
                      <Loader2 size={14} className="animate-spin" />
                      {t('grid.peekLoading', { defaultValue: 'Loading preview...' })}
                    </div>
                  ) : previewRows.length === 0 ? (
                    <div className="text-xs text-muted-foreground">
                      {t('grid.peekEmpty', { defaultValue: 'No matching row found' })}
                    </div>
                  ) : (
                    <div className="space-y-3">
                      {previewRows.map((row, rowIndex) => (
                        <div key={`${peekKey}-row-${rowIndex}`} className="space-y-1">
                          <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1.5fr)] gap-x-3 gap-y-1">
                            {previewColumns.map((col, colIndex) => {
                              const rawValue = row.values[colIndex];
                              const displayValue = formatValue(rawValue);
                              return (
                                <div key={`${peekKey}-${rowIndex}-${col.name}`} className="contents">
                                  <div className="text-xs text-muted-foreground truncate">
                                    {col.name}
                                  </div>
                                  <div
                                    className={cn(
                                      'text-xs font-mono text-foreground truncate',
                                      rawValue === null && 'italic text-muted-foreground'
                                    )}
                                  >
                                    {displayValue}
                                  </div>
                                </div>
                              );
                            })}
                          </div>
                          {rowIndex === 0 && extraColumns > 0 && (
                            <div className="text-xs text-muted-foreground">
                              {t('grid.peekColumnsMore', {
                                defaultValue: '+{{count}} more columns',
                                count: extraColumns,
                              })}
                            </div>
                          )}
                        </div>
                      ))}
                      {extraRows > 0 && (
                        <div className="text-xs text-muted-foreground">
                          {t('grid.peekRowsMore', {
                            defaultValue: '+{{count}} more rows',
                            count: extraRows,
                          })}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              </TooltipContent>
            </TooltipRoot>
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
    foreignKeyMap,
    buildPeekKey,
    peekCache,
    ensurePeekLoaded,
    getRelationLabel,
    resolveReferencedNamespace,
    onOpenRelatedTable,
    sessionId,
    namespace,
    tableName,
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
    if (!namespace || !tableName || !primaryKey || primaryKey.length === 0) return;

    const selectedRows = table.getSelectedRowModel().rows;
    if (selectedRows.length === 0) return;

    if (readOnly) {
      toast.error(t('environment.blocked'));
      return;
    }
    if (!mutationsSupported && !sandboxMode) {
      toast.error(t('grid.mutationsNotSupported'));
      return;
    }

    // Sandbox mode: add changes locally instead of executing
    if (sandboxMode && onSandboxDelete) {
      for (const row of selectedRows) {
        const pkData: Record<string, Value> = {};
        const oldValues: Record<string, Value> = {};
        let missingPk = false;

        for (const key of primaryKey) {
          if (row.original[key] === undefined) {
            missingPk = true;
            break;
          }
          pkData[key] = row.original[key];
        }

        if (missingPk) continue;

        // Capture all values for oldValues
        for (const [key, val] of Object.entries(row.original)) {
          oldValues[key] = val;
        }

        onSandboxDelete(pkData, oldValues);
      }
      table.resetRowSelection();
      return;
    }

    // Real deletion
    if (!sessionId) return;

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
    if (!mutationsSupported && !sandboxMode) {
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
                  const rowMeta = sandboxMode ? overlayResult.rowMetadata.get(virtualRow.index) : undefined;
                  const isInserted = rowMeta?.isInserted ?? false;
                  const isDeleted = rowMeta?.isDeleted ?? false;
                  const isModified = rowMeta?.isModified ?? false;

                  return (
                    <tr
                      key={row.id}
                      className={cn(
                        'border-b border-border hover:bg-muted/50 transition-colors',
                        row.getIsSelected() && 'bg-accent/10',
                        // Sandbox visual highlighting
                        isInserted && 'bg-success/10 hover:bg-success/15',
                        isDeleted && 'bg-error/10 hover:bg-error/15 line-through opacity-60',
                        isModified && !isInserted && !isDeleted && 'bg-warning/5 hover:bg-warning/10'
                      )}
                    >
                      {row.getVisibleCells().map(cell => {
                        const columnId = cell.column.id;
                        const isCellModified = rowMeta?.modifiedColumns.has(columnId) ?? false;

                        return (
                          <td
                            key={cell.id}
                            className={cn(
                              'px-3 py-1.5 max-w-xs',
                              // Highlight modified cells
                              isCellModified && !isInserted && !isDeleted && 'bg-warning/20'
                            )}
                            style={{ width: cell.column.getSize() }}
                          >
                            {/* Show NEW badge for inserted rows */}
                            {isInserted && cell.column.id === '__select' && (
                              <span className="inline-flex items-center px-1.5 py-0.5 text-[9px] font-bold rounded bg-success text-success-foreground mr-1.5">
                                {t('sandbox.row.new')}
                              </span>
                            )}
                            {flexRender(cell.column.columnDef.cell, cell.getContext())}
                          </td>
                        );
                      })}
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
