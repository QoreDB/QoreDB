// SPDX-License-Identifier: BUSL-1.1

import { Driver } from '@/lib/connection/drivers';
import { buildQualifiedTableName, quoteIdentifier } from '@/lib/ddl/identifiers';
import type { DiffResult, DiffRow } from '@/lib/diffUtils';
import { exactIntText, isExactInt } from '@/lib/query/exactInt';
import { quoteSqlString } from '@/lib/query/sqlStringLiteral';
import type { Namespace, Value } from '@/lib/tauri';

export interface DiffSqlPatchOptions {
  tableName: string;
  namespace?: Namespace | null;
  dialect?: Driver;
  keyColumns?: string[];
  maskedColumns?: string[];
  keyWarning?: string | null;
}

export interface DiffSqlPatchResult {
  sql: string;
  statementCount: number;
}

export type DiffSqlPatchBlocker =
  | 'targetNotTable'
  | 'maskedColumns'
  | 'missingKeys'
  | 'ambiguousKeys'
  | 'truncated'
  | 'noColumns';

export class DiffSqlPatchError extends Error {
  constructor(readonly code: DiffSqlPatchBlocker) {
    super(`Cannot generate an SQL patch: ${code}`);
    this.name = 'DiffSqlPatchError';
  }
}

/**
 * Why a patch cannot be generated, or `null`. A patch reconciles A toward B,
 * so it needs A to be a real table and every row of both sides to be keyed
 * by explicit, unambiguous, unmasked columns; a truncated comparison would
 * turn unread rows into spurious DELETE/INSERT statements.
 */
export function getDiffSqlPatchBlocker(
  diffResult: DiffResult,
  options: Pick<DiffSqlPatchOptions, 'tableName' | 'keyColumns' | 'maskedColumns' | 'keyWarning'>
): DiffSqlPatchBlocker | null {
  if (!options.tableName.trim()) return 'targetNotTable';
  if (diffResult.columns.length === 0 || diffResult.warnings.includes('noCommonColumns')) {
    return 'noColumns';
  }
  if (
    diffResult.warnings.includes('maskedColumns') ||
    diffResult.columns.some(col => col.masked) ||
    Boolean(options.maskedColumns && options.maskedColumns.length > 0)
  ) {
    return 'maskedColumns';
  }
  if (diffResult.warnings.includes('truncated')) return 'truncated';
  const columnNames = diffResult.columns.map(col => col.name);
  if (
    !options.keyColumns ||
    options.keyColumns.length === 0 ||
    options.keyColumns.some(name => !columnNames.includes(name)) ||
    diffResult.warnings.includes('missingKeyColumns')
  ) {
    return 'missingKeys';
  }
  if (options.keyWarning || diffResult.warnings.includes('ambiguousKeys')) return 'ambiguousKeys';
  return null;
}

function formatSqlLiteral(value: Value, driver: Driver): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'boolean') {
    if (driver === Driver.SqlServer || driver === Driver.AzureSql || driver === Driver.Synapse) {
      return value ? '1' : '0';
    }
    return value ? 'TRUE' : 'FALSE';
  }
  if (typeof value === 'number') {
    return Number.isFinite(value) ? String(value) : 'NULL';
  }
  if (isExactInt(value)) {
    const text = exactIntText(value).trim();
    return /^[+-]?\d+$/.test(text) ? text : quoteSqlString(text, driver);
  }
  if (typeof value === 'object') {
    return quoteSqlString(JSON.stringify(value), driver);
  }
  return quoteSqlString(String(value), driver);
}

function buildWhereClause(
  row: DiffRow,
  keyIndexes: number[],
  columnNames: string[],
  driver: Driver
): string {
  const cells = row.status === 'added' ? row.rightCells : row.leftCells;
  return keyIndexes
    .map(idx => {
      const col = quoteIdentifier(columnNames[idx], driver);
      const val = cells[idx]?.value ?? null;
      if (val === null || val === undefined) {
        return `${col} IS NULL`;
      }
      return `${col} = ${formatSqlLiteral(val, driver)}`;
    })
    .join(' AND ');
}

/**
 * Generates the `INSERT`/`UPDATE`/`DELETE` script that turns the left source
 * (A) into the right source (B). Apply it to A's table only.
 */
export function generateDiffSqlPatch(
  diffResult: DiffResult,
  options: DiffSqlPatchOptions
): DiffSqlPatchResult {
  const blocker = getDiffSqlPatchBlocker(diffResult, options);
  if (blocker) throw new DiffSqlPatchError(blocker);

  const tableName = options.tableName.trim();
  const driver = options.dialect ?? Driver.Postgres;
  const columnNames = diffResult.columns.map(col => col.name);
  const keyIndexes = (options.keyColumns ?? []).map(name => columnNames.indexOf(name));

  const qualifiedTable = options.namespace
    ? buildQualifiedTableName(options.namespace, tableName, driver)
    : quoteIdentifier(tableName, driver);

  const quotedAllCols = columnNames.map(name => quoteIdentifier(name, driver)).join(', ');
  const statements: string[] = [];

  for (const row of diffResult.rows) {
    if (row.status === 'unchanged') continue;

    if (row.status === 'added') {
      const values = row.rightCells.map(cell => formatSqlLiteral(cell.value, driver)).join(', ');
      statements.push(`INSERT INTO ${qualifiedTable} (${quotedAllCols}) VALUES (${values});`);
      continue;
    }

    if (row.status === 'removed') {
      const whereClause = buildWhereClause(row, keyIndexes, columnNames, driver);
      statements.push(`DELETE FROM ${qualifiedTable} WHERE ${whereClause};`);
      continue;
    }

    if (row.status === 'modified') {
      const assignments: string[] = [];
      for (let i = 0; i < columnNames.length; i++) {
        if (!row.rightCells[i]?.changed) continue;
        const col = quoteIdentifier(columnNames[i], driver);
        const lit = formatSqlLiteral(row.rightCells[i].value, driver);
        assignments.push(`${col} = ${lit}`);
      }
      if (assignments.length === 0) continue;
      const whereClause = buildWhereClause(row, keyIndexes, columnNames, driver);
      statements.push(
        `UPDATE ${qualifiedTable} SET ${assignments.join(', ')} WHERE ${whereClause};`
      );
    }
  }

  const headerLines = [`-- QoreDB Visual Data Diff SQL Patch (${statements.length} statement(s))`];

  return {
    sql: `${headerLines.join('\n')}${statements.length > 0 ? `\n${statements.join('\n')}\n` : '\n-- No changes.\n'}`,
    statementCount: statements.length,
  };
}
