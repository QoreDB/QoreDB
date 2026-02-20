// SPDX-License-Identifier: BUSL-1.1

/**
 * Data diff utilities for comparing query results
 */
import type { ColumnInfo, QueryResult, Row, Value } from './tauri';

export type DiffRowStatus = 'unchanged' | 'added' | 'removed' | 'modified';

export interface DiffCell {
  value: Value;
  changed: boolean;
}

export interface DiffRow {
  status: DiffRowStatus;
  leftCells: DiffCell[];
  rightCells: DiffCell[];
  rowKey: string;
}

export interface DiffResult {
  columns: ColumnInfo[];
  rows: DiffRow[];
  stats: DiffStats;
}

export interface DiffStats {
  unchanged: number;
  added: number;
  removed: number;
  modified: number;
  total: number;
}

/**
 * Generate a unique key for a row based on specified key columns or all columns
 */
function generateRowKey(row: Row, keyColumnIndexes: number[], fallbackIndex?: number): string {
  const validIndexes = keyColumnIndexes.filter(idx => idx >= 0);
  if (validIndexes.length === 0) {
    if (typeof fallbackIndex === 'number') {
      return `__row_index:${fallbackIndex}`;
    }
    return JSON.stringify(row.values);
  }

  return validIndexes.map(idx => JSON.stringify(row.values[idx])).join('|');
}

/**
 * Compare two values for equality
 */
function valuesEqual(a: Value, b: Value): boolean {
  if (a === b) return true;
  if (a === null || b === null) return false;
  if (typeof a === 'object' && typeof b === 'object') {
    return JSON.stringify(a) === JSON.stringify(b);
  }
  return false;
}

/**
 * Find common columns between two results
 */
export function findCommonColumns(left: QueryResult, right: QueryResult): ColumnInfo[] {
  const leftColNames = new Set(left.columns.map(c => c.name));
  return right.columns.filter(c => leftColNames.has(c.name));
}

/**
 * Get column indexes for a list of column names
 */
function getColumnIndexes(result: QueryResult, columnNames: string[]): number[] {
  return columnNames.map(name => result.columns.findIndex(c => c.name === name));
}

/**
 * Compare two query results and generate a diff
 * @param left Left side query result
 * @param right Right side query result
 * @param keyColumns Optional columns to use as row key for matching (uses all columns if not specified)
 */
export function compareResults(
  left: QueryResult,
  right: QueryResult,
  keyColumns?: string[]
): DiffResult {
  // Determine common columns to compare
  const commonColumns = findCommonColumns(left, right);
  const hasCommonColumns = commonColumns.length > 0;
  const outputColumns = hasCommonColumns ? commonColumns : left.columns;

  // Get column names for comparison
  const compareColumnNames = outputColumns.map(c => c.name);
  const leftIndexes = hasCommonColumns
    ? getColumnIndexes(left, compareColumnNames)
    : outputColumns.map((_, index) => index);
  const rightIndexes = hasCommonColumns
    ? getColumnIndexes(right, compareColumnNames)
    : outputColumns.map((_, index) => (index < right.columns.length ? index : -1));

  // Determine key columns for row matching
  let leftKeyIndexes: number[];
  let rightKeyIndexes: number[];

  if (hasCommonColumns) {
    const keyColNames = keyColumns ?? compareColumnNames;
    leftKeyIndexes = getColumnIndexes(left, keyColNames);
    rightKeyIndexes = getColumnIndexes(right, keyColNames);
  } else {
    leftKeyIndexes = leftIndexes;
    rightKeyIndexes = rightIndexes;
  }

  // Build maps of rows by key
  const leftRowMap = new Map<string, { row: Row; index: number }>();
  const rightRowMap = new Map<string, { row: Row; index: number }>();

  left.rows.forEach((row, index) => {
    const key = generateRowKey(row, leftKeyIndexes, index);
    leftRowMap.set(key, { row, index });
  });

  right.rows.forEach((row, index) => {
    const key = generateRowKey(row, rightKeyIndexes, index);
    rightRowMap.set(key, { row, index });
  });

  const diffRows: DiffRow[] = [];
  const processedRightKeys = new Set<string>();

  const stats: DiffStats = {
    unchanged: 0,
    added: 0,
    removed: 0,
    modified: 0,
    total: 0,
  };

  // Process left rows
  for (const [key, { row: leftRow }] of leftRowMap) {
    const rightEntry = rightRowMap.get(key);

    if (!rightEntry) {
      // Row only in left = removed
      const leftCells = leftIndexes.map(idx => ({
        value: idx >= 0 ? leftRow.values[idx] : null,
        changed: true,
      }));
      const rightCells = outputColumns.map(() => ({
        value: null,
        changed: true,
      }));

      diffRows.push({
        status: 'removed',
        leftCells,
        rightCells,
        rowKey: key,
      });
      stats.removed++;
    } else {
      // Row in both - compare values
      processedRightKeys.add(key);
      const rightRow = rightEntry.row;

      let hasChanges = false;
      const leftCells: DiffCell[] = [];
      const rightCells: DiffCell[] = [];

      for (let i = 0; i < outputColumns.length; i++) {
        const leftIdx = leftIndexes[i];
        const rightIdx = rightIndexes[i];

        const leftVal = leftIdx >= 0 ? leftRow.values[leftIdx] : null;
        const rightVal = rightIdx >= 0 ? rightRow.values[rightIdx] : null;

        const changed = !valuesEqual(leftVal, rightVal);
        if (changed) hasChanges = true;

        leftCells.push({ value: leftVal, changed });
        rightCells.push({ value: rightVal, changed });
      }

      const status = hasChanges ? 'modified' : 'unchanged';
      diffRows.push({
        status,
        leftCells,
        rightCells,
        rowKey: key,
      });

      if (hasChanges) {
        stats.modified++;
      } else {
        stats.unchanged++;
      }
    }
  }

  // Process right rows not in left = added
  for (const [key, { row: rightRow }] of rightRowMap) {
    if (processedRightKeys.has(key)) continue;

    const leftCells = outputColumns.map(() => ({
      value: null,
      changed: true,
    }));
    const rightCells = rightIndexes.map(idx => ({
      value: idx >= 0 ? rightRow.values[idx] : null,
      changed: true,
    }));

    diffRows.push({
      status: 'added',
      leftCells,
      rightCells,
      rowKey: key,
    });
    stats.added++;
  }

  stats.total = diffRows.length;

  // Sort: removed first, then modified, then unchanged, then added
  const statusOrder: Record<DiffRowStatus, number> = {
    removed: 0,
    modified: 1,
    unchanged: 2,
    added: 3,
  };

  diffRows.sort((a, b) => statusOrder[a.status] - statusOrder[b.status]);

  return {
    columns: outputColumns,
    rows: diffRows,
    stats,
  };
}

/**
 * Format a value for display in the diff view
 */
export function formatDiffValue(value: Value): string {
  if (value === null) return 'NULL';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

/**
 * Export diff results as CSV
 */
export function exportDiffAsCSV(diffResult: DiffResult): string {
  const { columns, rows } = diffResult;

  // Header row
  const header = ['_status', ...columns.map(c => c.name)];
  const lines: string[] = [header.map(escapeCSV).join(',')];

  // Data rows
  for (const row of rows) {
    const cells =
      row.status === 'removed'
        ? row.leftCells.map(c => formatDiffValue(c.value))
        : row.status === 'added'
          ? row.rightCells.map(c => formatDiffValue(c.value))
          : row.status === 'modified'
            ? row.rightCells.map((c, i) => {
                const oldVal = formatDiffValue(row.leftCells[i].value);
                const newVal = formatDiffValue(c.value);
                return row.leftCells[i].changed ? `${oldVal} → ${newVal}` : newVal;
              })
            : row.leftCells.map(c => formatDiffValue(c.value));

    const line = [row.status, ...cells].map(escapeCSV).join(',');
    lines.push(line);
  }

  return lines.join('\n');
}

/**
 * Escape a value for CSV
 */
function escapeCSV(value: string): string {
  if (value.includes(',') || value.includes('"') || value.includes('\n')) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

/**
 * Export diff results as JSON
 */
export function exportDiffAsJSON(diffResult: DiffResult): string {
  const { columns, rows, stats } = diffResult;

  const exportData = {
    columns: columns.map(c => ({ name: c.name, type: c.data_type })),
    stats,
    rows: rows.map(row => {
      const rowData: Record<string, unknown> = {
        _status: row.status,
        _key: row.rowKey,
      };

      if (row.status === 'removed') {
        columns.forEach((col, i) => {
          rowData[col.name] = row.leftCells[i].value;
        });
      } else if (row.status === 'added') {
        columns.forEach((col, i) => {
          rowData[col.name] = row.rightCells[i].value;
        });
      } else if (row.status === 'modified') {
        columns.forEach((col, i) => {
          if (row.leftCells[i].changed) {
            rowData[col.name] = {
              old: row.leftCells[i].value,
              new: row.rightCells[i].value,
            };
          } else {
            rowData[col.name] = row.leftCells[i].value;
          }
        });
      } else {
        columns.forEach((col, i) => {
          rowData[col.name] = row.leftCells[i].value;
        });
      }

      return rowData;
    }),
  };

  return JSON.stringify(exportData, null, 2);
}
