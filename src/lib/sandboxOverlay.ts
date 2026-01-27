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
 * Build a key from new values (for inserts)
 */
function buildValuesKey(values: Record<string, Value> | undefined, primaryKey: string[]): string {
  if (!values) return '';
  return primaryKey.map(col => JSON.stringify(values[col])).join('|');
}

/**
 * Check if a row matches a primary key
 */
function rowMatchesPk(
  row: Row,
  columnNames: string[],
  pk: RowData | undefined
): boolean {
  if (!pk?.columns) return false;

  return Object.entries(pk.columns).every(([col, value]) => {
    const colIndex = columnNames.indexOf(col);
    if (colIndex < 0) return false;
    const rowValue = row.values[colIndex];
    if (rowValue === value) return true;
    if (typeof rowValue === 'object' && typeof value === 'object') {
      return JSON.stringify(rowValue) === JSON.stringify(value);
    }
    return false;
  });
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
  for (const insert of inserts) {
    if (!insert.newValues) continue;

    // Build a row from the inserted values
    const values: Value[] = columnNames.map(col => insert.newValues![col] ?? null);
    const newIndex = newRows.length;

    // Insert at the beginning for visibility
    newRows.unshift({ values });

    // Adjust all existing metadata indices
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
 * Check if a row from original data would be affected by sandbox changes
 */
export function isRowAffected(
  row: Row,
  columnNames: string[],
  changes: SandboxChange[],
  primaryKey: string[]
): boolean {
  const updates = changes.filter(c => c.type === 'update');
  const deletes = changes.filter(c => c.type === 'delete');

  for (const change of [...updates, ...deletes]) {
    if (rowMatchesPk(row, columnNames, change.primaryKey)) {
      return true;
    }
  }

  return false;
}

/**
 * Get the original value of a cell before sandbox modifications
 */
export function getOriginalValue(
  rowIndex: number,
  columnName: string,
  overlayResult: OverlayResult,
  originalResult: QueryResult
): Value | undefined {
  const metadata = overlayResult.rowMetadata.get(rowIndex);

  if (!metadata?.change) return undefined;

  // For inserts, there's no original value
  if (metadata.isInserted) return undefined;

  // For updates, check oldValues
  if (metadata.isModified && metadata.change.oldValues) {
    return metadata.change.oldValues[columnName];
  }

  return undefined;
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
