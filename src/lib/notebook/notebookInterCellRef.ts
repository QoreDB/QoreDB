// SPDX-License-Identifier: BUSL-1.1

import type { NotebookCell } from './notebookTypes';

/**
 * Resolve inter-cell references in a source string.
 * Pattern: $label.column → values from the referenced cell's lastResult.
 *
 * Example: $users.id → (1, 2, 3)  (formatted for SQL IN clause)
 */
export function resolveInterCellReferences(source: string, cells: NotebookCell[]): string {
  return source.replace(/\$(\w+)\.(\w+)/g, (match, label: string, column: string) => {
    const cell = cells.find(c => c.config?.label === label);
    if (!cell?.lastResult || cell.lastResult.type !== 'table') return match;

    const { columns, rows } = cell.lastResult;
    if (!columns || !rows) return match;

    const colIndex = columns.findIndex(c => c.name === column);
    if (colIndex < 0) return match;

    const values = rows.map(r => {
      const val = r.values[colIndex];
      if (val === null || val === undefined) return 'NULL';
      if (typeof val === 'string') return `'${val.replace(/'/g, "''")}'`;
      return String(val);
    });

    if (values.length === 0) return 'NULL';
    if (values.length === 1) return values[0];
    return `(${values.join(', ')})`;
  });
}

/**
 * Find inter-cell references in a source string.
 * Returns list of {label, column} pairs.
 */
export function findInterCellReferences(source: string): Array<{ label: string; column: string }> {
  const refs: Array<{ label: string; column: string }> = [];
  for (const m of source.matchAll(/\$(\w+)\.(\w+)/g)) {
    refs.push({ label: m[1], column: m[2] });
  }
  return refs;
}
