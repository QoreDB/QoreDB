// SPDX-License-Identifier: Apache-2.0

import type { Driver } from '../connection/drivers';
import { buildAlterTableStatements } from './alterTableBuilders';
import type { BuildResult } from './createTable';
import type { AlterOp, ColumnDef, ForeignKeyDef, IndexDef, TableDefinition } from './types';

export interface DiffOptions {
  columnRenames?: Array<{ from: string; to: string }>;
  tableRename?: { from: string; to: string };
}

function colSizeKey(c: ColumnDef): string {
  return [c.length ?? '', c.precision ?? '', c.scale ?? ''].join('|');
}

function differs<T>(a: T | undefined | null, b: T | undefined | null): boolean {
  return (a ?? null) !== (b ?? null);
}

export function primaryKeyColumns(t: TableDefinition): string[] {
  return t.columns.filter(c => c.isPrimaryKey).map(c => c.name);
}

/** Everything that makes two same-named indexes actually different. */
function indexShape(i: IndexDef): string {
  return [i.columns.join(','), i.unique ? 'u' : '', i.method ?? '', i.where ?? ''].join('|');
}

function fkShape(fk: ForeignKeyDef): string {
  return [
    fk.columns.join(','),
    fk.refSchema ?? '',
    fk.refTable,
    fk.refColumns.join(','),
    fk.onDelete ?? '',
    fk.onUpdate ?? '',
  ].join('|');
}

/**
 * Diffs two table definitions into ALTER operations.
 *
 * The result is ordered by dependency, not by discovery: constraints and indexes
 * are dropped before the columns they cover, and re-created only once those
 * columns exist. `rename_table` comes last because every other statement is
 * built against the original table name.
 */
export function diffTableDefinitions(
  before: TableDefinition,
  after: TableDefinition,
  options: DiffOptions = {}
): AlterOp[] {
  const dropDeps: AlterOp[] = [];
  const dropCols: AlterOp[] = [];
  const renameCols: AlterOp[] = [];
  const colChanges: AlterOp[] = [];
  const addDeps: AlterOp[] = [];
  const tableMeta: AlterOp[] = [];

  const renamedTo = new Map<string, string>();
  const renamedFrom = new Map<string, string>();
  for (const r of options.columnRenames ?? []) {
    if (r.from === r.to) continue;
    renamedTo.set(r.from, r.to);
    renamedFrom.set(r.to, r.from);
  }

  const beforeCols = new Map(before.columns.map(c => [c.name, c]));
  const afterCols = new Map(after.columns.map(c => [c.name, c]));

  for (const [name] of beforeCols) {
    if (afterCols.has(name)) continue;
    if (renamedTo.has(name)) continue;
    dropCols.push({ kind: 'drop_column', columnName: name });
  }

  for (const [name, afterCol] of afterCols) {
    const originalName = renamedFrom.get(name) ?? name;
    const beforeCol = beforeCols.get(originalName);
    if (!beforeCol) {
      colChanges.push({ kind: 'add_column', column: afterCol });
      continue;
    }
    if (originalName !== name) {
      renameCols.push({ kind: 'rename_column', from: originalName, to: name });
    }
    if (beforeCol.type !== afterCol.type || colSizeKey(beforeCol) !== colSizeKey(afterCol)) {
      colChanges.push({
        kind: 'change_type',
        columnName: name,
        newType: afterCol.type,
        length: afterCol.length,
        precision: afterCol.precision,
        scale: afterCol.scale,
      });
    }
    if (beforeCol.nullable !== afterCol.nullable) {
      colChanges.push({ kind: 'set_nullable', columnName: name, nullable: afterCol.nullable });
    }
    if (differs(beforeCol.defaultValue, afterCol.defaultValue)) {
      colChanges.push({
        kind: 'set_default',
        columnName: name,
        defaultValue: afterCol.defaultValue,
      });
    }
    if ((beforeCol.comment ?? '') !== (afterCol.comment ?? '')) {
      colChanges.push({
        kind: 'set_column_comment',
        columnName: name,
        comment: afterCol.comment ?? '',
      });
    }
  }

  if ((before.comment ?? '') !== (after.comment ?? '')) {
    tableMeta.push({ kind: 'set_table_comment', comment: after.comment ?? '' });
  }

  const beforePk = primaryKeyColumns(before);
  const afterPk = primaryKeyColumns(after);
  if (beforePk.join(',') !== afterPk.join(',')) {
    if (beforePk.length > 0) {
      dropDeps.push({ kind: 'drop_primary_key', name: before.primaryKeyName });
    }
    if (afterPk.length > 0) {
      addDeps.push({ kind: 'add_primary_key', columns: afterPk });
    }
  }

  // Identity is the name when there is one, but a same-named key whose
  // definition changed still has to be replaced — so compare the shape too.
  const fkIdentity = (fk: ForeignKeyDef) => (fk.name ? `name:${fk.name}` : `cols:${fkShape(fk)}`);

  const beforeFks = new Map((before.foreignKeys ?? []).map(fk => [fkIdentity(fk), fk]));
  const afterFks = new Map((after.foreignKeys ?? []).map(fk => [fkIdentity(fk), fk]));
  for (const [key, fk] of beforeFks) {
    const next = afterFks.get(key);
    if (!next || fkShape(next) !== fkShape(fk)) {
      dropDeps.push({ kind: 'drop_foreign_key', name: fk.name ?? key });
    }
  }
  for (const [key, fk] of afterFks) {
    const prev = beforeFks.get(key);
    if (!prev || fkShape(prev) !== fkShape(fk)) {
      addDeps.push({ kind: 'add_foreign_key', foreignKey: fk });
    }
  }

  const beforeIdx = new Map((before.indexes ?? []).map(i => [i.name, i]));
  const afterIdx = new Map((after.indexes ?? []).map(i => [i.name, i]));
  for (const [name, idx] of beforeIdx) {
    const next = afterIdx.get(name);
    if (!next || indexShape(next) !== indexShape(idx)) {
      dropDeps.push({ kind: 'drop_index', name });
    }
  }
  for (const [name, idx] of afterIdx) {
    const prev = beforeIdx.get(name);
    if (!prev || indexShape(prev) !== indexShape(idx)) {
      addDeps.push({ kind: 'add_index', index: idx });
    }
  }

  const checkKey = (c: { name?: string; expression: string }) =>
    c.name ? `name:${c.name}` : `expr:${c.expression}`;
  const beforeChecks = new Map((before.checks ?? []).map(c => [checkKey(c), c]));
  const afterChecks = new Map((after.checks ?? []).map(c => [checkKey(c), c]));
  for (const [key, c] of beforeChecks) {
    if (!afterChecks.has(key) && c.name) {
      dropDeps.push({ kind: 'drop_check', name: c.name });
    }
  }
  for (const [key, c] of afterChecks) {
    if (!beforeChecks.has(key)) addDeps.push({ kind: 'add_check', check: c });
  }

  const rename: AlterOp[] =
    options.tableRename && options.tableRename.from !== options.tableRename.to
      ? [{ kind: 'rename_table', newName: options.tableRename.to }]
      : [];

  return [
    ...dropDeps,
    ...dropCols,
    ...renameCols,
    ...colChanges,
    ...addDeps,
    ...tableMeta,
    ...rename,
  ];
}

export function buildAlterTableSQL(
  table: TableDefinition,
  ops: AlterOp[],
  driver: Driver
): BuildResult {
  return buildAlterTableStatements(table, ops, driver);
}
