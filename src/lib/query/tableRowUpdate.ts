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
import { isExactInt } from './exactInt';
import { estimatePayloadBytes, TAB_PAYLOAD_BUDGET_BYTES } from './payloadSize';

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

interface RowUpdateTarget {
  row: Row;
  columns: ColumnInfo[];
  keyIndices: number[];
  protectedIndices: number[];
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

  const protectedColumns = new Set([
    ...primaryKey,
    ...(sortColumn ? [sortColumn] : []),
    ...(filters?.map(filter => filter.column) ?? []),
  ]);
  if (protectedColumns.has(update.column)) return null;
  const protectedIndices = [...protectedColumns].map(name =>
    columns.findIndex(c => c.name === name)
  );
  if (protectedIndices.some(index => index < 0 || columns[index].masked)) return null;
  const keyIndices = primaryKey.map(name => columns.findIndex(c => c.name === name));
  if (
    keyIndices.some(
      (index, i) =>
        isBinaryType(columns[index].data_type) || !usableKey(update.primaryKey[primaryKey[i]])
    )
  ) {
    return null;
  }
  const matches = rows.filter(row =>
    keyIndices.every((index, i) => sameValue(row.values[index], update.primaryKey[primaryKey[i]]))
  );
  if (matches.length !== 1 || matches[0].values.length !== columns.length) return null;
  return { row: matches[0], columns, keyIndices, protectedIndices };
}

export function reconcileTableRowUpdate(
  rows: Row[],
  target: RowUpdateTarget,
  result: PaginatedQueryResult,
  payloadBytes: number,
  budget = TAB_PAYLOAD_BUDGET_BYTES
): { rows: Row[]; payloadBytes: number } | null {
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
  if (target.protectedIndices.some(i => !sameValue(target.row.values[i], replacement.values[i]))) {
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
  const updated = rows.slice();
  updated[rowIndex] = replacement;
  return { rows: updated, payloadBytes: nextBytes };
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
