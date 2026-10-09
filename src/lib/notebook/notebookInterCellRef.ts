// SPDX-License-Identifier: BUSL-1.1

import type { CellType, NotebookCell, NotebookVariable, QoreNotebook } from './notebookTypes';
import { extractVariableReferences, substituteVariables } from './notebookVariables';

function referenceCell(cells: NotebookCell[], label: string, column: string) {
  const matches = cells.filter(c => c.config?.label === label);
  const cell = matches[0];
  if (matches.length !== 1 || cell.executionState !== 'success') return undefined;
  const result = cell.lastResult;
  if (result?.type !== 'table' || result.truncated || !result.rows || !result.columns)
    return undefined;
  const columns = result.columns.filter(c => c.name === column);
  if (columns.length !== 1 || columns[0].masked) return undefined;
  return cell;
}

export function findUnavailableReference(
  source: string,
  cells: NotebookCell[],
  cellType: CellType = 'sql'
): string | undefined {
  const ref = findInterCellReferences(source).find(({ label, column }) => {
    // Mongo field paths use the same spelling; only known labels are notebook references.
    if (cellType === 'mongo' && !cells.some(cell => cell.config?.label === label)) return false;
    return !referenceCell(cells, label, column);
  });
  return ref ? `$${ref.label}.${ref.column}` : undefined;
}

/** Propagate invalidation through labels, including removed and ambiguous sources. */
export function invalidateNotebookDependents(prev: QoreNotebook, next: QoreNotebook): QoreNotebook {
  const labels = new Set<string>();
  const staleIds = new Set<string>();
  const changedVariables = new Set(
    [...Object.keys(prev.variables), ...Object.keys(next.variables)].filter(
      name => prev.variables[name] !== next.variables[name]
    )
  );
  const previous = new Map(prev.cells.map(cell => [cell.id, cell]));
  for (const cell of next.cells) {
    const old = previous.get(cell.id);
    previous.delete(cell.id);
    const inputChanged =
      old &&
      (old.source !== cell.source ||
        old.type !== cell.type ||
        old.config?.label !== cell.config?.label ||
        old.config?.namespace !== cell.config?.namespace);
    const variableChanged = extractVariableReferences(cell.source).some(name =>
      changedVariables.has(name)
    );
    if (inputChanged || variableChanged) staleIds.add(cell.id);
    if (
      !old ||
      inputChanged ||
      variableChanged ||
      old.lastResult !== cell.lastResult ||
      old.executionState !== cell.executionState
    ) {
      if (old?.config?.label) labels.add(old.config.label);
      if (cell.config?.label) labels.add(cell.config.label);
    }
  }
  for (const cell of previous.values()) {
    if (cell.config?.label) labels.add(cell.config.label);
  }
  let changed = true;
  while (changed) {
    changed = false;
    for (const cell of next.cells) {
      if (staleIds.has(cell.id)) continue;
      const depends =
        findInterCellReferences(cell.source).some(ref => labels.has(ref.label)) ||
        (cell.type === 'chart' && labels.has(cell.config?.chartConfig?.sourceLabel ?? ''));
      if (!depends) continue;
      staleIds.add(cell.id);
      if (cell.config?.label) labels.add(cell.config.label);
      changed = true;
    }
  }
  return {
    ...next,
    cells: next.cells.map(cell =>
      staleIds.has(cell.id) && (cell.lastResult || cell.executionState === 'running')
        ? { ...cell, executionState: 'stale' }
        : cell
    ),
  };
}

/** Resolve only tokens present in the source, never text inserted by another token. */
export function resolveNotebookReferences(
  source: string,
  variables: Record<string, NotebookVariable>,
  cells: NotebookCell[]
): string {
  return source.replace(/\{\{\w+\}\}|(?<!\$)\$\w+(\.\w+)?/g, (match, column) =>
    column ? resolveInterCellReferences(match, cells) : substituteVariables(match, variables)
  );
}

/**
 * Resolve inter-cell references in a source string.
 * Pattern: $label.column → values from the referenced cell's lastResult.
 *
 * Example: $users.id → (1, 2, 3)  (formatted for SQL IN clause)
 */
export function resolveInterCellReferences(source: string, cells: NotebookCell[]): string {
  return source.replace(/(?<!\$)\$(\w+)\.(\w+)/g, (match, label: string, column: string) => {
    const cell = referenceCell(cells, label, column);
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

export function findInterCellReferences(source: string): Array<{ label: string; column: string }> {
  const refs: Array<{ label: string; column: string }> = [];
  for (const m of source.matchAll(/(?<!\$)\$(\w+)\.(\w+)/g)) {
    refs.push({ label: m[1], column: m[2] });
  }
  return refs;
}
