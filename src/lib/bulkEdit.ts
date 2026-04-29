// SPDX-License-Identifier: Apache-2.0

/**
 * Pure helpers for the Bulk Edit feature.
 *
 * Builds `SandboxChangeDto` updates from a user-defined plan applied to a
 * selection of rows. Shared by the dialog, the SQL preview, and the apply
 * flow — all consumers go through `buildBulkEditChanges`.
 *
 * No backend calls happen here.
 */

import type { RowData as GridRowData } from '@/components/Grid/utils/dataGridUtils';
import type { SandboxChangeDto } from './sandboxTypes';
import type { Namespace, TableSchema, RowData as TauriRowData, Value } from './tauri';

/** Maximum rows allowed in Core (Pro is unlimited). */
export const BULK_EDIT_CORE_LIMIT = 5;

/** Hard cap to prevent runaway operations even for Pro users. */
export const BULK_EDIT_HARD_LIMIT = 1000;

/** Operation applied uniformly across the selected rows. */
export type BulkEditOperation = 'set_value' | 'set_null';

export interface BulkEditPlan {
  column: string;
  operation: BulkEditOperation;
  /** Raw user input, ignored when `operation === 'set_null'`. */
  value: string;
}

export type BulkEditError =
  | 'noRows'
  | 'tooManyRows'
  | 'noPrimaryKey'
  | 'noColumn'
  | 'unknownColumn'
  | 'cannotEditPk'
  | 'notNullable'
  | 'requiresPro';

export interface BulkEditValidation {
  ok: boolean;
  errors: BulkEditError[];
}

interface ValidateArgs {
  plan: BulkEditPlan;
  rowCount: number;
  tableSchema: TableSchema | null;
  primaryKey: string[] | undefined;
  hasPro: boolean;
}

/**
 * Validates a plan + selection. Errors are returned as i18n keys (under
 * `bulkEdit.errors.*`) so the caller can render them directly.
 */
export function validateBulkEdit({
  plan,
  rowCount,
  tableSchema,
  primaryKey,
  hasPro,
}: ValidateArgs): BulkEditValidation {
  const errors: BulkEditError[] = [];

  if (rowCount === 0) errors.push('noRows');
  if (rowCount > BULK_EDIT_HARD_LIMIT) errors.push('tooManyRows');
  if (!hasPro && rowCount > BULK_EDIT_CORE_LIMIT) errors.push('requiresPro');

  if (!primaryKey || primaryKey.length === 0) errors.push('noPrimaryKey');

  if (!plan.column) {
    errors.push('noColumn');
  } else if (tableSchema) {
    const col = tableSchema.columns.find(c => c.name === plan.column);
    if (!col) {
      errors.push('unknownColumn');
    } else {
      if (col.is_primary_key) errors.push('cannotEditPk');
      if (plan.operation === 'set_null' && !col.nullable) errors.push('notNullable');
    }
  }

  return { ok: errors.length === 0, errors };
}

/**
 * Coerces a raw string from the user input into a typed `Value` based on
 * the column's data_type. Falls back to the raw string when no specific
 * coercion applies — backend / driver does final type validation.
 */
export function coerceValueForColumn(raw: string, dataType: string): Value {
  const dt = dataType.toLowerCase();

  if (dt.includes('bool')) {
    if (raw === 'true' || raw === '1' || raw === 't') return true;
    if (raw === 'false' || raw === '0' || raw === 'f') return false;
    return raw;
  }

  if (
    dt.includes('int') ||
    dt.includes('serial') ||
    dt.includes('numeric') ||
    dt.includes('decimal') ||
    dt.includes('real') ||
    dt.includes('double') ||
    dt.includes('float')
  ) {
    if (raw.trim() === '') return raw;
    const n = Number(raw);
    return Number.isFinite(n) ? n : raw;
  }

  return raw;
}

/**
 * Lists columns eligible for Bulk Edit: present in the schema and not part
 * of the primary key. Returns names in their schema-declared order.
 */
export function eligibleColumnsForBulkEdit(tableSchema: TableSchema | null): string[] {
  if (!tableSchema) return [];
  return tableSchema.columns.filter(c => !c.is_primary_key).map(c => c.name);
}

interface BuildArgs {
  plan: BulkEditPlan;
  rows: GridRowData[];
  namespace: Namespace;
  tableName: string;
  primaryKey: string[];
  tableSchema: TableSchema | null;
}

/**
 * Produces one `SandboxChangeDto` per eligible row. Rows where the primary
 * key value is missing are silently skipped (callers should validate counts
 * via `validateBulkEdit` first).
 */
export function buildBulkEditChanges({
  plan,
  rows,
  namespace,
  tableName,
  primaryKey,
  tableSchema,
}: BuildArgs): SandboxChangeDto[] {
  const dataType = tableSchema?.columns.find(c => c.name === plan.column)?.data_type ?? 'text';

  const newValue: Value =
    plan.operation === 'set_null' ? null : coerceValueForColumn(plan.value, dataType);

  const dtos: SandboxChangeDto[] = [];

  for (const row of rows) {
    const pkRecord: Record<string, Value> = {};
    let missing = false;
    for (const pk of primaryKey) {
      const v = row[pk];
      if (v === undefined) {
        missing = true;
        break;
      }
      pkRecord[pk] = v as Value;
    }
    if (missing) continue;

    const tauriPk: TauriRowData = { columns: pkRecord };

    dtos.push({
      change_type: 'update',
      namespace,
      table_name: tableName,
      primary_key: tauriPk,
      old_values: { [plan.column]: (row[plan.column] ?? null) as Value },
      new_values: { [plan.column]: newValue },
    });
  }

  return dtos;
}
