// SPDX-License-Identifier: Apache-2.0

import type { VirtualItem } from '@tanstack/react-virtual';
import type { UseTranslationOptions } from 'react-i18next';
import type { Environment, Namespace, QueryResult, Value } from '@/lib/tauri';

export interface DocumentResultsProps {
  result: QueryResult;
  sessionId?: string;
  database?: string;
  collection?: string;
  environment?: Environment;
  readOnly?: boolean;
  connectionName?: string;
  connectionDatabase?: string;
  onEditDocument?: (doc: Record<string, unknown>, idValue?: Value) => void;
  onRowsDeleted?: () => void;
  exportQuery?: string;
  exportNamespace?: Namespace;

  serverSideTotalRows?: number;
  serverSidePage?: number;
  serverSidePageSize?: number;
  onServerPageChange?: (page: number) => void;
  onServerPageSizeChange?: (pageSize: number) => void;
}

export interface DocumentRowItemProps {
  virtualRow: VirtualItem;
  doc: DocumentRow;
  measureElement: (element: Element | null) => void;
  readOnly: boolean;
  t: (key: string, options?: UseTranslationOptions<string>) => string;
  onCopy: (row: DocumentRow) => void;
  onEdit: (doc: Record<string, unknown>, idValue?: Value) => void;
  onDelete: (row: DocumentRow) => void;
}

export type DocumentRow = {
  doc: Record<string, unknown> | unknown;
  idValue?: Value;
  idLabel?: string;
  json: string;
  search: string;
};

const DOCUMENT_COLUMN = 'document';

function coerceIdValue(id: unknown): Value | undefined {
  if (id && typeof id === 'object' && !Array.isArray(id)) {
    const oid = (id as Record<string, unknown>).$oid;
    if (typeof oid === 'string') {
      return oid;
    }
  }
  if (
    id === null ||
    typeof id === 'string' ||
    typeof id === 'number' ||
    typeof id === 'boolean' ||
    typeof id === 'object'
  ) {
    return id as Value;
  }
  return undefined;
}

function formatIdLabel(id: unknown): string {
  if (id === undefined) return '-';
  if (typeof id === 'string' || typeof id === 'number' || typeof id === 'boolean') {
    return String(id);
  }
  if (id && typeof id === 'object' && !Array.isArray(id)) {
    const oid = (id as Record<string, unknown>).$oid;
    if (typeof oid === 'string') return oid;
  }
  return JSON.stringify(id);
}

function normalizeDocument(
  result: QueryResult,
  rowValues: Value[]
): Record<string, unknown> | unknown {
  if (result.columns.length === 1 && result.columns[0]?.name === DOCUMENT_COLUMN) {
    return rowValues[0] ?? {};
  }

  const data: Record<string, unknown> = {};
  result.columns.forEach((col, idx) => {
    data[col.name] = rowValues[idx];
  });
  return data;
}

export { coerceIdValue, formatIdLabel, normalizeDocument, DOCUMENT_COLUMN };
