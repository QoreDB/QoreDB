// SPDX-License-Identifier: Apache-2.0

/**
 * Apply sandbox changes as an overlay on top of QueryResult data, providing a
 * view of what the data would look like after applying changes.
 */

import type { Namespace, QueryResult, Row, RowData, TableSchema, Value } from '../tauri';
import type { SandboxChange, SandboxDeleteDisplay, SandboxRowMetadata } from './sandboxTypes';

export interface OverlayResult {
  result: QueryResult;
  /** Metadata for each row (keyed by row index in result) */
  rowMetadata: Map<number, SandboxRowMetadata>;
  stats: {
    insertedRows: number;
    modifiedRows: number;
    deletedRows: number;
    hiddenRows: number;
  };
}

function buildRowKey(row: Row, primaryKeyIndices: number[]): string {
  return primaryKeyIndices.map(i => JSON.stringify(row.values[i])).join('|');
}

function buildPkKey(pk: RowData | undefined, primaryKey: string[]): string {
  if (!pk?.columns) return '';
  return primaryKey.map(col => JSON.stringify(pk.columns[col])).join('|');
}

export function applyOverlay(
  result: QueryResult,
  changes: SandboxChange[],
  schema: TableSchema | null,
  namespace: Namespace,
  tableName: string,
  options: {
    deleteDisplay: SandboxDeleteDisplay;
    primaryKey?: string[];
  }
): OverlayResult {
  const { deleteDisplay, primaryKey: pkOverride } = options;
  const primaryKey = pkOverride ?? schema?.primary_key ?? [];
  const columnNames = result.columns.map(c => c.name);

  const tableChanges = changes.filter(
    c =>
      c.tableName === tableName &&
      c.namespace.database === namespace.database &&
      c.namespace.schema === namespace.schema
  );

  const inserts = tableChanges.filter(c => c.type === 'insert');
  const updates = tableChanges.filter(c => c.type === 'update');
  const deletes = tableChanges.filter(c => c.type === 'delete');

  const primaryKeyIndices = primaryKey.map(col => columnNames.indexOf(col)).filter(i => i >= 0);
  const hasValidPk = primaryKeyIndices.length > 0 && primaryKeyIndices.length === primaryKey.length;

  const deleteKeys = new Set<string>();
  const updateMap = new Map<string, SandboxChange>();

  if (hasValidPk) {
    for (const del of deletes) {
      const key = buildPkKey(del.primaryKey, primaryKey);
      if (key) deleteKeys.add(key);
    }

    for (const upd of updates) {
      const key = buildPkKey(upd.primaryKey, primaryKey);
      if (key) updateMap.set(key, upd);
    }
  }

  const newRows: Row[] = [];
  const rowMetadata = new Map<number, SandboxRowMetadata>();
  let hiddenCount = 0;

  for (const row of result.rows) {
    const rowKey = hasValidPk ? buildRowKey(row, primaryKeyIndices) : '';

    if (rowKey && deleteKeys.has(rowKey)) {
      if (deleteDisplay === 'hidden') {
        hiddenCount++;
        continue;
      }
      // strikethrough: keep the row but mark it
      const metadata: SandboxRowMetadata = {
        isModified: false,
        isInserted: false,
        isDeleted: true,
        modifiedColumns: new Set(),
        change: deletes.find(d => buildPkKey(d.primaryKey, primaryKey) === rowKey),
      };
      const newIndex = newRows.length;
      newRows.push(row);
      rowMetadata.set(newIndex, metadata);
      continue;
    }

    const updateChange = rowKey ? updateMap.get(rowKey) : undefined;
    if (updateChange?.newValues) {
      const newValues = [...row.values];
      const modifiedColumns = new Set<string>();

      for (const [col, value] of Object.entries(updateChange.newValues)) {
        const colIndex = columnNames.indexOf(col);
        if (colIndex >= 0) {
          newValues[colIndex] = value;
          modifiedColumns.add(col);
        }
      }

      const newIndex = newRows.length;
      newRows.push({ values: newValues });
      rowMetadata.set(newIndex, {
        isModified: true,
        isInserted: false,
        isDeleted: false,
        modifiedColumns,
        change: updateChange,
      });
      continue;
    }

    newRows.push(row);
  }

  for (const insert of inserts) {
    if (!insert.newValues) continue;

    const values: Value[] = columnNames.map(col => insert.newValues?.[col] ?? null);

    // Insert at the beginning for visibility
    newRows.unshift({ values });

    // Existing rows shifted by one, so bump every metadata index to match.
    const adjustedMetadata = new Map<number, SandboxRowMetadata>();
    for (const [idx, meta] of rowMetadata) {
      adjustedMetadata.set(idx + 1, meta);
    }

    adjustedMetadata.set(0, {
      isModified: false,
      isInserted: true,
      isDeleted: false,
      modifiedColumns: new Set(Object.keys(insert.newValues)),
      change: insert,
    });

    rowMetadata.clear();
    for (const [idx, meta] of adjustedMetadata) {
      rowMetadata.set(idx, meta);
    }
  }

  return {
    result: {
      ...result,
      rows: newRows,
    },
    rowMetadata,
    stats: {
      insertedRows: inserts.length,
      modifiedRows: updates.length,
      deletedRows: deletes.length,
      hiddenRows: hiddenCount,
    },
  };
}

export function getRowMetadata(
  rowIndex: number,
  overlayResult: OverlayResult
): SandboxRowMetadata | undefined {
  return overlayResult.rowMetadata.get(rowIndex);
}

export function isCellModified(
  rowIndex: number,
  columnName: string,
  overlayResult: OverlayResult
): boolean {
  const metadata = overlayResult.rowMetadata.get(rowIndex);
  if (!metadata) return false;
  return metadata.modifiedColumns.has(columnName);
}

/** Empty overlay result for when sandbox is disabled. */
export function emptyOverlayResult(result: QueryResult): OverlayResult {
  return {
    result,
    rowMetadata: new Map(),
    stats: {
      insertedRows: 0,
      modifiedRows: 0,
      deletedRows: 0,
      hiddenRows: 0,
    },
  };
}

export interface ChangeDiff {
  column: string;
  oldValue: Value;
  newValue: Value;
}

export function getChangeDiff(change: SandboxChange): ChangeDiff[] {
  if (change.type === 'insert') {
    return Object.entries(change.newValues ?? {}).map(([column, newValue]) => ({
      column,
      oldValue: null,
      newValue,
    }));
  }

  if (change.type === 'update') {
    return Object.entries(change.newValues ?? {}).map(([column, newValue]) => ({
      column,
      oldValue: change.oldValues?.[column] ?? null,
      newValue,
    }));
  }

  if (change.type === 'delete') {
    return Object.entries(change.oldValues ?? {}).map(([column, oldValue]) => ({
      column,
      oldValue,
      newValue: null,
    }));
  }

  return [];
}
