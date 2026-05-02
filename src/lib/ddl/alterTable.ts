// SPDX-License-Identifier: Apache-2.0

import type { Driver } from '../connection/drivers';
import { buildAlterTableStatements } from './alterTableBuilders';
import type { BuildResult } from './createTable';
import type { AlterOp, ColumnDef, TableDefinition } from './types';

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

export function diffTableDefinitions(
  before: TableDefinition,
  after: TableDefinition,
  options: DiffOptions = {}
): AlterOp[] {
  const ops: AlterOp[] = [];

  if (options.tableRename && options.tableRename.from !== options.tableRename.to) {
    ops.push({ kind: 'rename_table', newName: options.tableRename.to });
  }

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
    ops.push({ kind: 'drop_column', columnName: name });
  }

  for (const [name, afterCol] of afterCols) {
    const originalName = renamedFrom.get(name) ?? name;
    const beforeCol = beforeCols.get(originalName);
    if (!beforeCol) {
      ops.push({ kind: 'add_column', column: afterCol });
      continue;
    }
    if (originalName !== name) {
      ops.push({ kind: 'rename_column', from: originalName, to: name });
    }
    if (beforeCol.type !== afterCol.type || colSizeKey(beforeCol) !== colSizeKey(afterCol)) {
      ops.push({
        kind: 'change_type',
        columnName: name,
        newType: afterCol.type,
        length: afterCol.length,
        precision: afterCol.precision,
        scale: afterCol.scale,
      });
    }
    if (beforeCol.nullable !== afterCol.nullable) {
      ops.push({ kind: 'set_nullable', columnName: name, nullable: afterCol.nullable });
    }
    if (differs(beforeCol.defaultValue, afterCol.defaultValue)) {
      ops.push({
        kind: 'set_default',
        columnName: name,
        defaultValue: afterCol.defaultValue,
      });
    }
    if ((beforeCol.comment ?? '') !== (afterCol.comment ?? '')) {
      ops.push({
        kind: 'set_column_comment',
        columnName: name,
        comment: afterCol.comment ?? '',
      });
    }
  }

  if ((before.comment ?? '') !== (after.comment ?? '')) {
    ops.push({ kind: 'set_table_comment', comment: after.comment ?? '' });
  }

  const fkKey = (fk: {
    name?: string;
    columns: string[];
    refTable: string;
    refColumns: string[];
  }) =>
    fk.name
      ? `name:${fk.name}`
      : `cols:${fk.columns.join(',')}->${fk.refTable}(${fk.refColumns.join(',')})`;

  const beforeFks = new Map((before.foreignKeys ?? []).map(fk => [fkKey(fk), fk]));
  const afterFks = new Map((after.foreignKeys ?? []).map(fk => [fkKey(fk), fk]));
  for (const [key, fk] of beforeFks) {
    if (!afterFks.has(key)) {
      ops.push({ kind: 'drop_foreign_key', name: fk.name ?? key });
    }
  }
  for (const [key, fk] of afterFks) {
    if (!beforeFks.has(key)) {
      ops.push({ kind: 'add_foreign_key', foreignKey: fk });
    }
  }

  const beforeIdx = new Map((before.indexes ?? []).map(i => [i.name, i]));
  const afterIdx = new Map((after.indexes ?? []).map(i => [i.name, i]));
  for (const [name] of beforeIdx) {
    if (!afterIdx.has(name)) ops.push({ kind: 'drop_index', name });
  }
  for (const [name, idx] of afterIdx) {
    if (!beforeIdx.has(name)) ops.push({ kind: 'add_index', index: idx });
  }

  const checkKey = (c: { name?: string; expression: string }) =>
    c.name ? `name:${c.name}` : `expr:${c.expression}`;
  const beforeChecks = new Map((before.checks ?? []).map(c => [checkKey(c), c]));
  const afterChecks = new Map((after.checks ?? []).map(c => [checkKey(c), c]));
  for (const [key, c] of beforeChecks) {
    if (!afterChecks.has(key) && c.name) {
      ops.push({ kind: 'drop_check', name: c.name });
    }
  }
  for (const [key, c] of afterChecks) {
    if (!beforeChecks.has(key)) ops.push({ kind: 'add_check', check: c });
  }

  return ops;
}

export function buildAlterTableSQL(
  table: TableDefinition,
  ops: AlterOp[],
  driver: Driver
): BuildResult {
  return buildAlterTableStatements(table, ops, driver);
}
