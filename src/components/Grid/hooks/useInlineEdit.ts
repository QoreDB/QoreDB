// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import type { TableCellUpdateHandler } from '@/lib/query/tableRowUpdate';
import { type Environment, type Namespace, updateRow, type Value } from '@/lib/tauri';
import type { RowData } from '../utils/dataGridUtils';
import { useValueParsing } from './useValueParsing';

export interface UseInlineEditProps {
  sessionId?: string;
  namespace?: Namespace;
  tableName?: string;
  primaryKey?: string[];
  environment?: Environment;
  readOnly?: boolean;
  mutationsSupported?: boolean;
  sandboxMode?: boolean;
  columnTypeMap: Map<string, string>;
  maskedColumns?: Set<string>;
  onSandboxUpdate?: (
    pk: Record<string, Value>,
    oldValues: Record<string, Value>,
    newValues: Record<string, Value>
  ) => void;
  onRowsUpdated?: () => void;
  onUpdateCell?: TableCellUpdateHandler;
}

export interface UseInlineEditReturn {
  editingCell: { rowId: string; columnId: string } | null;
  editingValue: string;
  setEditingValue: (value: string) => void;
  editInputRef: React.RefObject<HTMLInputElement | null>;
  isUpdating: boolean;
  startInlineEdit: (row: RowData, rowId: string, columnId: string, value: Value) => void;
  commitInlineEdit: () => Promise<void>;
  cancelInlineEdit: () => void;
  inlineEditAvailable: boolean;
  // Refs for external access (needed by cell rendering)
  editingCellRef: React.RefObject<{ rowId: string; columnId: string } | null>;
  editingValueRef: React.RefObject<string>;
  // Update confirmation state (for production environment)
  updateConfirmOpen: boolean;
  setUpdateConfirmOpen: (open: boolean) => void;
  pendingUpdate: {
    row: RowData;
    columnId: string;
    value: Value;
    originalValue: Value;
  } | null;
  setPendingUpdate: (
    update: {
      row: RowData;
      columnId: string;
      value: Value;
      originalValue: Value;
    } | null
  ) => void;
  performInlineUpdate: (
    payload: {
      row: RowData;
      columnId: string;
      value: Value;
      originalValue: Value;
    },
    acknowledgedDangerous?: boolean
  ) => Promise<void>;
}

export function useInlineEdit({
  sessionId,
  namespace,
  tableName,
  primaryKey,
  environment = 'development',
  readOnly = false,
  mutationsSupported = true,
  sandboxMode = false,
  columnTypeMap,
  maskedColumns,
  onSandboxUpdate,
  onRowsUpdated,
  onUpdateCell,
}: UseInlineEditProps): UseInlineEditReturn {
  const { t } = useTranslation();
  const { getEditableValue, parseInputValue, valuesEqual } = useValueParsing();

  const [editingCell, setEditingCell] = useState<{ rowId: string; columnId: string } | null>(null);
  const [, setEditingValue] = useState('');
  const [isUpdating, setIsUpdating] = useState(false);
  const updatingRef = useRef(false);

  // Confirmation dialog state (for production)
  const [updateConfirmOpen, setUpdateConfirmOpen] = useState(false);
  const [pendingUpdate, setPendingUpdate] = useState<{
    row: RowData;
    columnId: string;
    value: Value;
    originalValue: Value;
  } | null>(null);

  // Refs for synchronous access in callbacks
  const editInputRef = useRef<HTMLInputElement>(null);
  const skipCommitRef = useRef(false);
  const editingCellRef = useRef<{ rowId: string; columnId: string } | null>(null);
  const editingRowRef = useRef<RowData | null>(null);
  const editingValueRef = useRef('');
  const editingInitialValueRef = useRef('');
  const editingOriginalValueRef = useRef<Value | undefined>(undefined);

  const hasInlineEditContext = Boolean(sessionId && namespace && tableName);
  const hasPrimaryKey = Boolean(primaryKey && primaryKey.length > 0);
  const inlineEditAvailable = hasInlineEditContext && hasPrimaryKey;

  useEffect(() => {
    if (!editingCell) return;
    requestAnimationFrame(() => {
      editInputRef.current?.focus();
      editInputRef.current?.select();
    });
  }, [editingCell]);

  const resetEditingState = useCallback(() => {
    setEditingCell(null);
    setEditingValue('');
    editingCellRef.current = null;
    editingRowRef.current = null;
    editingValueRef.current = '';
    editingInitialValueRef.current = '';
    editingOriginalValueRef.current = undefined;
  }, []);

  const editContext = JSON.stringify([
    sessionId,
    namespace?.database,
    namespace?.schema,
    tableName,
  ]);
  const editContextRef = useRef(editContext);
  useEffect(() => {
    if (editContextRef.current !== editContext) {
      editContextRef.current = editContext;
      resetEditingState();
      setPendingUpdate(null);
      setUpdateConfirmOpen(false);
    }
  }, [editContext, resetEditingState]);

  const startInlineEdit = useCallback(
    (row: RowData, rowId: string, columnId: string, currentValue: Value) => {
      if (updatingRef.current) return;
      skipCommitRef.current = false;
      if (editingCellRef.current?.rowId === rowId && editingCellRef.current.columnId === columnId) {
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
      // The cell holds the masked text: saving it would overwrite the real value.
      if (maskedColumns?.has(columnId)) {
        toast.error(t('grid.masking.readOnly'));
        return;
      }

      const displayValue = getEditableValue(currentValue);
      const cellRef = { rowId, columnId };
      setEditingCell(cellRef);
      setEditingValue(displayValue);
      editingCellRef.current = cellRef;
      editingRowRef.current = row;
      editingValueRef.current = displayValue;
      editingInitialValueRef.current = displayValue;
      editingOriginalValueRef.current = currentValue;
    },
    [
      hasInlineEditContext,
      hasPrimaryKey,
      readOnly,
      mutationsSupported,
      maskedColumns,
      t,
      getEditableValue,
    ]
  );

  const performInlineUpdate = useCallback(
    async (
      payload: { row: RowData; columnId: string; value: Value; originalValue: Value },
      acknowledgedDangerous = false
    ) => {
      if (updatingRef.current) return;
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

      if (!sessionId) {
        toast.error(t('grid.updateNoPrimaryKey'));
        return;
      }

      updatingRef.current = true;
      setIsUpdating(true);
      try {
        const res = onUpdateCell
          ? await onUpdateCell(
              {
                primaryKey: pkData,
                column: payload.columnId,
                value: payload.value,
              },
              acknowledgedDangerous
            )
          : await updateRow(
              sessionId,
              namespace.database,
              namespace.schema,
              tableName,
              { columns: pkData },
              { columns: { [payload.columnId]: payload.value } },
              acknowledgedDangerous
            );
        if (res.success) {
          toast.success(t('grid.updateSuccess'));
          if (!onUpdateCell) onRowsUpdated?.();
        } else {
          toast.error(t('grid.updateError'));
        }
      } catch {
        toast.error(t('grid.updateError'));
      } finally {
        updatingRef.current = false;
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
      onUpdateCell,
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
      await performInlineUpdate(payload, false);
    } else {
      setPendingUpdate(payload);
      setUpdateConfirmOpen(true);
    }
  }, [
    columnTypeMap,
    environment,
    performInlineUpdate,
    resetEditingState,
    parseInputValue,
    valuesEqual,
  ]);

  const cancelInlineEdit = useCallback(() => {
    skipCommitRef.current = true;
    resetEditingState();
  }, [resetEditingState]);

  // Setter for editing value that updates both state and ref
  const setEditingValueSync = useCallback((value: string) => {
    setEditingValue(value);
    editingValueRef.current = value;
  }, []);

  return {
    editingCell,
    editingValue: editingValueRef.current,
    setEditingValue: setEditingValueSync,
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
  };
}
