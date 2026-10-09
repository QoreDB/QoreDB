// SPDX-License-Identifier: BUSL-1.1

import type { ColumnInfo, QueryResult, Row, Value } from './tauri';

export type DiffRowStatus = 'unchanged' | 'added' | 'removed' | 'modified';
export type DiffWarning =
  | 'ambiguousKeys'
  | 'uncomparedColumns'
  | 'noCommonColumns'
  | 'missingKeyColumns'
  | 'maskedColumns'
  | 'truncated';

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
  incomplete: boolean;
  warnings: DiffWarning[];
}

export interface DiffStats {
  unchanged: number;
  added: number;
  removed: number;
  modified: number;
  total: number;
}

function serializeValue(value: Value): string {
  return JSON.stringify(value, (_, nested) =>
    nested && typeof nested === 'object' && !Array.isArray(nested)
      ? Object.fromEntries(
          Object.keys(nested)
            .sort()
            .map(key => [key, nested[key]])
        )
      : nested
  );
}

function valuesEqual(a: Value, b: Value): boolean {
  if (a === b) return true;
  if (a === null || b === null) return false;
  if (typeof a === 'object' && typeof b === 'object') {
    return serializeValue(a) === serializeValue(b);
  }
  return false;
}

export function findCommonColumns(left: QueryResult, right: QueryResult): ColumnInfo[] {
  const leftColNames = new Set(left.columns.map(c => c.name));
  return right.columns.filter(c => leftColNames.has(c.name));
}

function getColumnIndexes(result: QueryResult, columnNames: string[]): number[] {
  return columnNames.map(name => result.columns.findIndex(c => c.name === name));
}

/**
 * Compare two query results and generate a diff
 * @param left Left side query result
 * @param right Right side query result
 * @param keyColumns Optional columns to use as row key for matching (uses all columns if not specified)
 * Duplicate keys match identical rows first, then remaining rows in source order.
 */
export function compareResults(
  left: QueryResult,
  right: QueryResult,
  keyColumns?: string[],
  options: { truncated?: boolean } = {}
): DiffResult {
  const warnings: DiffWarning[] = [];
  const commonColumns = findCommonColumns(left, right);
  const hasCommonColumns = commonColumns.length > 0;
  const outputColumns = hasCommonColumns ? commonColumns : left.columns;

  if (!hasCommonColumns) warnings.push('noCommonColumns');
  else if (
    commonColumns.length !== left.columns.length ||
    commonColumns.length !== right.columns.length ||
    new Set(commonColumns.map(column => column.name)).size !== commonColumns.length
  ) {
    warnings.push('uncomparedColumns');
  }
  if ([...left.columns, ...right.columns].some(column => column.masked)) {
    warnings.push('maskedColumns');
  }
  if (options.truncated) warnings.push('truncated');

  const compareColumnNames = outputColumns.map(c => c.name);
  const leftIndexes = hasCommonColumns
    ? getColumnIndexes(left, compareColumnNames)
    : outputColumns.map((_, index) => index);
  const rightIndexes = hasCommonColumns
    ? getColumnIndexes(right, compareColumnNames)
    : outputColumns.map((_, index) => (index < right.columns.length ? index : -1));

  let leftKeyIndexes: number[];
  let rightKeyIndexes: number[];

  if (hasCommonColumns) {
    const keyColNames = keyColumns?.length ? keyColumns : compareColumnNames;
    leftKeyIndexes = getColumnIndexes(left, keyColNames);
    rightKeyIndexes = getColumnIndexes(right, keyColNames);
  } else {
    leftKeyIndexes = leftIndexes;
    rightKeyIndexes = rightIndexes;
  }

  if (
    keyColumns?.some(
      name => !left.columns.some(c => c.name === name) || !right.columns.some(c => c.name === name)
    )
  ) {
    warnings.push('missingKeyColumns');
    leftKeyIndexes = leftIndexes;
    rightKeyIndexes = rightIndexes;
  }

  function groupRows(rows: Row[], indexes: number[]): Map<string, Row[]> {
    const groups = new Map<string, Row[]>();
    const validIndexes = indexes.filter(idx => idx >= 0);
    rows.forEach((row, index) => {
      const key =
        validIndexes.length > 0
          ? serializeValue(validIndexes.map(idx => row.values[idx]))
          : `__row_index:${index}`;
      const group = groups.get(key);
      if (group) group.push(row);
      else groups.set(key, [row]);
    });
    return groups;
  }

  const leftRowMap = groupRows(left.rows, leftKeyIndexes);
  const rightRowMap = groupRows(right.rows, rightKeyIndexes);
  if (
    keyColumns?.length &&
    [...leftRowMap.values(), ...rightRowMap.values()].some(rows => rows.length > 1)
  ) {
    warnings.push('ambiguousKeys');
  }

  const diffRows: DiffRow[] = [];

  const stats: DiffStats = {
    unchanged: 0,
    added: 0,
    removed: 0,
    modified: 0,
    total: 0,
  };

  function appendRow(key: string, occurrence: number, leftRow?: Row, rightRow?: Row) {
    let hasChanges = false;
    const leftCells: DiffCell[] = [];
    const rightCells: DiffCell[] = [];
    for (let i = 0; i < outputColumns.length; i++) {
      const leftVal = leftRow && leftIndexes[i] >= 0 ? leftRow.values[leftIndexes[i]] : null;
      const rightVal = rightRow && rightIndexes[i] >= 0 ? rightRow.values[rightIndexes[i]] : null;
      const changed = !leftRow || !rightRow || !valuesEqual(leftVal, rightVal);
      if (changed) hasChanges = true;
      leftCells.push({ value: leftVal, changed });
      rightCells.push({ value: rightVal, changed });
    }
    const status = !leftRow
      ? 'added'
      : !rightRow
        ? 'removed'
        : hasChanges
          ? 'modified'
          : 'unchanged';
    diffRows.push({ status, leftCells, rightCells, rowKey: JSON.stringify([key, occurrence]) });
    stats[status]++;
  }

  const keys = new Set([...leftRowMap.keys(), ...rightRowMap.keys()]);
  for (const key of keys) {
    const leftRows = leftRowMap.get(key) ?? [];
    const rightRows = rightRowMap.get(key) ?? [];
    const buckets = new Map<string, { indexes: number[]; used: number }>();
    rightRows.forEach((row, index) => {
      const signature = serializeValue(
        rightIndexes.map(idx => (idx >= 0 ? row.values[idx] : null))
      );
      const bucket = buckets.get(signature);
      if (bucket) bucket.indexes.push(index);
      else buckets.set(signature, { indexes: [index], used: 0 });
    });
    const matched = new Set<number>();
    const matches = leftRows.map(row => {
      const signature = serializeValue(leftIndexes.map(idx => (idx >= 0 ? row.values[idx] : null)));
      const bucket = buckets.get(signature);
      if (!bucket || bucket.used === bucket.indexes.length) return undefined;
      const index = bucket.indexes[bucket.used++];
      matched.add(index);
      return rightRows[index];
    });
    const remaining = rightRows.filter((_, index) => !matched.has(index));
    let nextRight = 0;
    leftRows.forEach((row, index) => {
      appendRow(key, index, row, matches[index] ?? remaining[nextRight++]);
    });
    for (; nextRight < remaining.length; nextRight++) {
      appendRow(key, leftRows.length + nextRight, undefined, remaining[nextRight]);
    }
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
    incomplete: warnings.length > 0,
    warnings,
  };
}

export function formatDiffValue(value: Value): string {
  if (value === null) return 'NULL';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

export function exportDiffAsCSV(diffResult: DiffResult): string {
  const { columns, rows } = diffResult;

  const header = ['_status', '_comparison_incomplete', ...columns.map(c => c.name)];
  const lines: string[] = [header.map(escapeCSV).join(',')];

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

    const line = [row.status, String(diffResult.incomplete), ...cells].map(escapeCSV).join(',');
    lines.push(line);
  }

  return lines.join('\n');
}

function escapeCSV(value: string): string {
  if (value.includes(',') || value.includes('"') || value.includes('\n') || value.includes('\r')) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

export function exportDiffAsJSON(diffResult: DiffResult): string {
  const { columns, rows, stats } = diffResult;

  const exportData = {
    incomplete: diffResult.incomplete,
    warnings: diffResult.warnings,
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
