// SPDX-License-Identifier: Apache-2.0

import type { TableColumn } from '@/lib/tauri';

const TEXT_TYPE = /char|text|string|clob|uuid|enum|json|name|citext/i;
const BINARY_TYPE = /blob|bytea|binary|image|tsvector|tsquery|geometry|geography/i;

/**
 * Columns a full-text search should cover by default.
 *
 * Text columns only: casting every numeric, date and boolean column to text
 * multiplies the predicates without matching anything a user is looking for.
 * Falls back to every non-binary column when a table has no text column at all,
 * so searching a lookup table of integers still works.
 */
export function defaultSearchColumns(columns: TableColumn[] | undefined): string[] {
  if (!columns || columns.length === 0) return [];

  const textual = columns.filter(col => TEXT_TYPE.test(col.data_type)).map(col => col.name);
  if (textual.length > 0) return textual;

  return columns.filter(col => !BINARY_TYPE.test(col.data_type)).map(col => col.name);
}
