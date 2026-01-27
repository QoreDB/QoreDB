/**
 * Sandbox Overlay Utilities
 *
 * Functions to apply sandbox changes as an overlay on top of QueryResult data.
 * This provides a view of what the data would look like after applying changes.
 */

import { QueryResult, Value, Row, RowData, TableSchema, Namespace } from './tauri';
import { SandboxChange, SandboxRowMetadata, SandboxDeleteDisplay } from './sandboxTypes';

/**
 * Result of applying sandbox overlay
 */
export interface OverlayResult {
  /** Modified query result with sandbox changes applied */
  result: QueryResult;
  /** Metadata for each row (keyed by row index in result) */
  rowMetadata: Map<number, SandboxRowMetadata>;
  /** Statistics about the overlay */
  stats: {
    insertedRows: number;
    modifiedRows: number;
    deletedRows: number;
    hiddenRows: number;
  };
}

/**
 * Build a key to identify a row by its primary key values
 */
function buildRowKey(row: Row, primaryKeyIndices: number[]): string {
  return primaryKeyIndices.map(i => JSON.stringify(row.values[i])).join('|');
}

/**
 * Build a key from a RowData object
 */
function buildPkKey(pk: RowData | undefined, primaryKey: string[]): string {
  if (!pk?.columns) return '';
  return primaryKey.map(col => JSON.stringify(pk.columns[col])).join('|');
}

/**
 * Apply sandbox changes as an overlay on QueryResult
 */
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

  // Filter changes for this table
  const tableChanges = changes.filter(
    c =>
      c.tableName === tableName &&
      c.namespace.database === namespace.database &&
      c.namespace.schema === namespace.schema
  );

  // Separate by type
  const inserts = tableChanges.filter(c => c.type === 'insert');
  const updates = tableChanges.filter(c => c.type === 'update');
  const deletes = tableChanges.filter(c => c.type === 'delete');

  // Build lookup maps
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

  // Process existing rows
  const newRows: Row[] = [];
  const rowMetadata = new Map<number, SandboxRowMetadata>();
  let hiddenCount = 0;

  for (const row of result.rows) {
    const rowKey = hasValidPk ? buildRowKey(row, primaryKeyIndices) : '';

    // Check if deleted
    if (rowKey && deleteKeys.has(rowKey)) {
      if (deleteDisplay === 'hidden') {
        hiddenCount++;
        continue; // Skip this row
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

    // Check if updated
    const updateChange = rowKey ? updateMap.get(rowKey) : undefined;
    if (updateChange?.newValues) {
      // Apply updates to the row
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

    // Unchanged row
    newRows.push(row);
  }

  // Add inserted rows at the beginning
  // Build all insert rows first
  const insertRows: Array<{ values: Value[]; change: SandboxChange }> = [];
  for (const insert of inserts) {
    if (!insert.newValues) continue;
    const values: Value[] = columnNames.map(col => insert.newValues![col] ?? null);
    insertRows.push({ values, change: insert });
  }

  // Prepend all insert rows at once
  newRows.unshift(...insertRows.map(ir => ({ values: ir.values })));

  // Rebuild metadata map with adjusted indices
  const adjustedMetadata = new Map<number, SandboxRowMetadata>();
  
  // Add metadata for inserted rows
  for (let i = 0; i < insertRows.length; i++) {
    adjustedMetadata.set(i, {
      isModified: false,
      isInserted: true,
      isDeleted: false,
      modifiedColumns: new Set(Object.keys(insertRows[i].change.newValues!)),
      change: insertRows[i].change,
    });
  }
  
  // Shift existing metadata by the number of inserts
  const offset = insertRows.length;
  for (const [idx, meta] of rowMetadata) {
    adjustedMetadata.set(idx + offset, meta);
  }

  rowMetadata.clear();
  for (const [idx, meta] of adjustedMetadata) {
    rowMetadata.set(idx, meta);
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

/**
 * Get metadata for a specific row
 */
export function getRowMetadata(
  rowIndex: number,
  overlayResult: OverlayResult
): SandboxRowMetadata | undefined {
  return overlayResult.rowMetadata.get(rowIndex);
}

/**
 * Check if a cell is modified
 */
export function isCellModified(
  rowIndex: number,
  columnName: string,
  overlayResult: OverlayResult
): boolean {
  const metadata = overlayResult.rowMetadata.get(rowIndex);
  if (!metadata) return false;
  return metadata.modifiedColumns.has(columnName);
}

/**
 * Create empty overlay result for when sandbox is disabled
 */
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

/**
 * Compute a diff summary for display
 */
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
