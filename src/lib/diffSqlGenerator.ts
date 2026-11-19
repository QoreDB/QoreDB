// SPDX-License-Identifier: BUSL-1.1

import { Driver } from '@/lib/connection/drivers';
import { buildQualifiedTableName, quoteIdentifier } from '@/lib/ddl/identifiers';
import type { DiffResult, DiffRow } from '@/lib/diffUtils';
import { exactIntText, isExactInt } from '@/lib/query/exactInt';
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
    return /^[+-]?\d+$/.test(text) ? text : `'${text.replace(/'/g, "''")}'`;
  }
  if (typeof value === 'object') {
    const json = JSON.stringify(value);
    return `'${json.replace(/'/g, "''")}'`;
  }
  const clean = String(value).replace(/\0/g, '');
  return `'${clean.replace(/'/g, "''")}'`;
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
 * Generates an idempotent-oriented SQL patch (`INSERT`, `UPDATE`, `DELETE`)
 * that reconciles the left source (A) toward the right source (B).
 *
 * Refuses explicitly when columns are masked or when key matching is missing
 * or ambiguous so masked values (`***`) or duplicate keys cannot corrupt data.
 */
export function generateDiffSqlPatch(
  diffResult: DiffResult,
  options: DiffSqlPatchOptions
): DiffSqlPatchResult {
  const tableName = options.tableName.trim();
  if (!tableName) {
    throw new Error('Target table name is required to generate an SQL patch');
  }

  const hasMasked =
    diffResult.warnings.includes('maskedColumns') ||
    diffResult.columns.some(col => col.masked) ||
    Boolean(options.maskedColumns && options.maskedColumns.length > 0);
  if (hasMasked) {
    throw new Error('Cannot generate an SQL patch when the diff contains masked columns');
  }

  if (
    options.keyWarning ||
    diffResult.warnings.includes('ambiguousKeys') ||
    diffResult.warnings.includes('missingKeyColumns') ||
    diffResult.warnings.includes('noCommonColumns')
  ) {
    throw new Error('Cannot generate an SQL patch when key columns are missing or ambiguous');
  }

  const driver = options.dialect ?? Driver.Postgres;
  const columnNames = diffResult.columns.map(col => col.name);
  if (columnNames.length === 0) {
    throw new Error('Cannot generate an SQL patch without columns');
  }

  const requestedKeys =
    options.keyColumns && options.keyColumns.length > 0 ? options.keyColumns : columnNames;
  const keyIndexes = requestedKeys.map(name => columnNames.indexOf(name));
  if (keyIndexes.some(idx => idx < 0)) {
    throw new Error('One or more key columns are missing from the diff result');
  }

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
  if (diffResult.warnings.includes('truncated')) {
    headerLines.push('-- WARNING: Comparison was truncated; review statements before applying.');
  }

  return {
    sql: `${headerLines.join('\n')}${statements.length > 0 ? `\n${statements.join('\n')}\n` : '\n-- No changes.\n'}`,
    statementCount: statements.length,
  };
}
