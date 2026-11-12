// SPDX-License-Identifier: Apache-2.0

import { isBinaryType } from '@/lib/binaryUtils';
import { type MutationResponse, updateRow } from '@/lib/tauri/mutations';
import {
  type ColumnFilter,
  type OrderingGuarantee,
  type PaginatedQueryResult,
  queryTable,
} from '@/lib/tauri/query';
import type { ColumnInfo, Namespace, Row, Value } from '@/lib/tauri/types';
import { exactIntText, isExactInt } from './exactInt';
import { estimatePayloadBytes, TAB_PAYLOAD_BUDGET_BYTES } from './payloadSize';

export type RowAnchorReason = 'filter_mismatch' | 'order_changed' | 'inserted';

export interface TableCellUpdate {
  primaryKey: Record<string, Value>;
  column: string;
  value: Value;
}

export type TableCellUpdateHandler = (
  update: TableCellUpdate,
  acknowledgedDangerous?: boolean
) => Promise<MutationResponse>;

export interface TableRowUpdateScope {
  rows: Row[];
  columns: ColumnInfo[];
  primaryKey?: string[];
  sortColumn?: string;
  filters?: ColumnFilter[];
  searchTerm?: string;
  orderingGuarantee: OrderingGuarantee;
}

export interface RowUpdateTarget {
  row: Row;
  columns: ColumnInfo[];
  keyIndices: number[];
  sortIndex: number | null;
  filterSpecs: Array<{ index: number; filter: ColumnFilter }>;
}

export interface ReconciledRowUpdate {
  rows: Row[];
  payloadBytes: number;
  rowIndex: number;
  anchorReason?: Exclude<RowAnchorReason, 'inserted'>;
}

function usableKey(value: Value | undefined): boolean {
  if (typeof value === 'string' || typeof value === 'boolean') return true;
  if (typeof value === 'number') {
    return Number.isFinite(value) && (!Number.isInteger(value) || Number.isSafeInteger(value));
  }
  return isExactInt(value) && /^-?\d+$/.test(value.$qoreInt);
}

function sameValue(a: Value, b: Value): boolean {
  return a === b || (isExactInt(a) && isExactInt(b) && a.$qoreInt === b.$qoreInt);
}

function valueToText(value: Value): string {
  if (value === null) return '';
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  if (isExactInt(value)) return exactIntText(value);
  return JSON.stringify(value);
}

function toBigIntIfIntegral(value: Value): bigint | null {
  if (isExactInt(value) && /^-?\d+$/.test(value.$qoreInt)) {
    return BigInt(value.$qoreInt);
  }
  if (typeof value === 'number' && Number.isInteger(value) && Number.isSafeInteger(value)) {
    return BigInt(value);
  }
  if (typeof value === 'string' && /^-?\d+$/.test(value.trim())) {
    return BigInt(value.trim());
  }
  return null;
}

function compareValues(a: Value, b: Value): number | null {
  if (sameValue(a, b)) return 0;
  const bigA = toBigIntIfIntegral(a);
  const bigB = toBigIntIfIntegral(b);
  if (bigA !== null && bigB !== null) {
    return bigA < bigB ? -1 : bigA > bigB ? 1 : 0;
  }
  const numA =
    typeof a === 'number' ? a : typeof a === 'string' && a.trim() !== '' ? Number(a) : Number.NaN;
  const numB =
    typeof b === 'number' ? b : typeof b === 'string' && b.trim() !== '' ? Number(b) : Number.NaN;
  if (Number.isFinite(numA) && Number.isFinite(numB)) {
    return numA < numB ? -1 : numA > numB ? 1 : 0;
  }
  if (typeof a === 'boolean' && typeof b === 'string') {
    const normB = b.trim().toLowerCase();
    if (normB === 'true' || normB === 'false') {
      return a === (normB === 'true') ? 0 : a ? 1 : -1;
    }
  }
  const strA = valueToText(a);
  const strB = valueToText(b);
  return strA < strB ? -1 : strA > strB ? 1 : 0;
}

function likePatternToRegExp(pattern: string): RegExp {
  let regexSource = '^';
  for (let i = 0; i < pattern.length; i++) {
    const ch = pattern[i];
    if (ch === '%') {
      regexSource += '[\\s\\S]*';
    } else if (ch === '_') {
      regexSource += '[\\s\\S]';
    } else {
      regexSource += ch.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    }
  }
  regexSource += '$';
  return new RegExp(regexSource, 'i');
}

export function matchesColumnFilter(value: Value, filter: ColumnFilter): boolean {
  if (filter.operator === 'is_null') return value === null;
  if (filter.operator === 'is_not_null') return value !== null;
  if (value === null || filter.value === null) return false;

  switch (filter.operator) {
    case 'eq':
      return compareValues(value, filter.value) === 0;
    case 'neq':
      return compareValues(value, filter.value) !== 0;
    case 'gt': {
      const cmp = compareValues(value, filter.value);
      return cmp !== null && cmp > 0;
    }
    case 'gte': {
      const cmp = compareValues(value, filter.value);
      return cmp !== null && cmp >= 0;
    }
    case 'lt': {
      const cmp = compareValues(value, filter.value);
      return cmp !== null && cmp < 0;
    }
    case 'lte': {
      const cmp = compareValues(value, filter.value);
      return cmp !== null && cmp <= 0;
    }
    case 'like':
      return likePatternToRegExp(valueToText(filter.value)).test(valueToText(value));
    case 'regex':
      try {
        return new RegExp(valueToText(filter.value), filter.options?.regex_flags ?? '').test(
          valueToText(value)
        );
      } catch {
        return false;
      }
    case 'text':
      return valueToText(value).toLowerCase().includes(valueToText(filter.value).toLowerCase());
    default:
      return false;
  }
}

export function prepareTableRowUpdate(
  scope: TableRowUpdateScope,
  update: TableCellUpdate
): RowUpdateTarget | null {
  const { columns, primaryKey, rows, sortColumn, filters, searchTerm, orderingGuarantee } = scope;
  if (!primaryKey?.length || searchTerm || orderingGuarantee !== 'stable') return null;
  if (new Set(columns.map(column => column.name)).size !== columns.length) return null;
  if (Object.keys(update.primaryKey).length !== primaryKey.length) return null;
  const edited = columns.find(column => column.name === update.column);
  if (!edited || edited.masked) return null;

  // Primary key edits change row identity and still require a full reload.
  // Edits to sort or filter columns are allowed and anchored locally (§9.6).
  if (primaryKey.includes(update.column)) return null;

  const keyIndices = primaryKey.map(name => columns.findIndex(c => c.name === name));
  if (
    keyIndices.some(
      (index, i) =>
        index < 0 ||
        columns[index].masked ||
        isBinaryType(columns[index].data_type) ||
        !usableKey(update.primaryKey[primaryKey[i]])
    )
  ) {
    return null;
  }

  let sortIndex: number | null = null;
  if (sortColumn) {
    const idx = columns.findIndex(c => c.name === sortColumn);
    if (idx < 0 || columns[idx].masked) return null;
    sortIndex = idx;
  }

  const filterSpecs: Array<{ index: number; filter: ColumnFilter }> = [];
  for (const filter of filters ?? []) {
    const idx = columns.findIndex(c => c.name === filter.column);
    if (idx < 0 || columns[idx].masked) return null;
    filterSpecs.push({ index: idx, filter });
  }

  const matches = rows.filter(row =>
    keyIndices.every((index, i) => sameValue(row.values[index], update.primaryKey[primaryKey[i]]))
  );
  if (matches.length !== 1 || matches[0].values.length !== columns.length) return null;
  return { row: matches[0], columns, keyIndices, sortIndex, filterSpecs };
}

export function reconcileTableRowUpdate(
  rows: Row[],
  target: RowUpdateTarget,
  result: PaginatedQueryResult,
  payloadBytes: number,
  budget = TAB_PAYLOAD_BUDGET_BYTES
): ReconciledRowUpdate | null {
  const fresh = result.result;
  if (
    result.has_more ||
    fresh.rows.length !== 1 ||
    fresh.columns.length !== target.columns.length
  ) {
    return null;
  }
  if (
    fresh.columns.some((column, i) => {
      const original = target.columns[i];
      return (
        column.name !== original.name ||
        column.data_type !== original.data_type ||
        column.nullable !== original.nullable ||
        Boolean(column.masked) !== Boolean(original.masked)
      );
    })
  ) {
    return null;
  }
  const replacement = fresh.rows[0];
  if (replacement.values.length !== target.columns.length) return null;
  // Only primary key changes invalidate identity and trigger a full reload.
  if (target.keyIndices.some(i => !sameValue(target.row.values[i], replacement.values[i]))) {
    return null;
  }

  // Locate by key again against the current pages, which may have grown during
  // the round-trip. A changed row object means another result already replaced it.
  let rowIndex = -1;
  for (let i = 0; i < rows.length; i++) {
    if (
      target.keyIndices.every(index => sameValue(rows[i].values[index], target.row.values[index]))
    ) {
      if (rowIndex !== -1 || rows[i] !== target.row) return null;
      rowIndex = i;
    }
  }
  if (rowIndex === -1) return null;

  const nextBytes =
    payloadBytes - estimatePayloadBytes([target.row]) + estimatePayloadBytes([replacement]);
  if (nextBytes > budget) return null;

  let anchorReason: Exclude<RowAnchorReason, 'inserted'> | undefined;
  const filterMismatch = target.filterSpecs.some(
    spec => !matchesColumnFilter(replacement.values[spec.index], spec.filter)
  );
  if (filterMismatch) {
    anchorReason = 'filter_mismatch';
  } else if (
    target.sortIndex !== null &&
    !sameValue(target.row.values[target.sortIndex], replacement.values[target.sortIndex])
  ) {
    anchorReason = 'order_changed';
  }

  const updated = rows.slice();
  updated[rowIndex] = replacement;
  return { rows: updated, payloadBytes: nextBytes, rowIndex, anchorReason };
}

export function reconcileTableRowInsert(
  rows: Row[],
  columns: ColumnInfo[],
  insertedValues: Record<string, Value>,
  payloadBytes: number,
  budget = TAB_PAYLOAD_BUDGET_BYTES
): { rows: Row[]; payloadBytes: number; rowIndex: number; anchorReason: 'inserted' } | null {
  if (columns.length === 0) return null;
  const newRow: Row = {
    values: columns.map(col => (col.name in insertedValues ? insertedValues[col.name] : null)),
  };
  const nextBytes = payloadBytes + estimatePayloadBytes([newRow]);
  if (nextBytes > budget) return null;
  return {
    rows: [newRow, ...rows],
    payloadBytes: nextBytes,
    rowIndex: 0,
    anchorReason: 'inserted',
  };
}

interface InlineUpdateView {
  isCurrent: () => boolean;
  replace: (target: RowUpdateTarget, result: PaginatedQueryResult) => boolean;
  reload: () => void;
}

/** The write is issued once. A failed canonical read never becomes a failed write. */
export async function updateTableRow(
  sessionId: string,
  namespace: Namespace,
  tableName: string,
  update: TableCellUpdate,
  target: RowUpdateTarget | null,
  view: InlineUpdateView,
  acknowledgedDangerous = false
): Promise<MutationResponse> {
  const response = await updateRow(
    sessionId,
    namespace.database,
    namespace.schema,
    tableName,
    { columns: update.primaryKey },
    { columns: { [update.column]: update.value } },
    acknowledgedDangerous
  );
  if (!response.success || !view.isCurrent()) return response;

  if (target && (response.result?.affected_rows ?? 1) <= 1) {
    try {
      const fresh = await queryTable(
        sessionId,
        namespace,
        tableName,
        {
          page: 1,
          page_size: 2,
          count_mode: 'none',
          filters: Object.entries(update.primaryKey).map(([column, value]) => ({
            column,
            operator: 'eq',
            value,
          })),
        },
        true
      );
      if (!view.isCurrent()) return response;
      if (fresh.success && fresh.result && view.replace(target, fresh.result)) return response;
    } catch {
      // The mutation already succeeded. Reload with a distinct message rather
      // than suggesting the user retry and possibly execute triggers twice.
    }
  }
  if (view.isCurrent()) view.reload();
  return response;
}
