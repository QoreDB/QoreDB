// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@tauri-apps/api/core';
import type { QueryResult, Value } from './types';

// ============================================
// MUTATIONS
// ============================================

export interface RowData {
  columns: Record<string, Value>;
}

export interface MutationResponse {
  success: boolean;
  result?: QueryResult;
  error?: string;
}

export async function insertRow(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  table: string,
  data: RowData,
  acknowledgedDangerous?: boolean
): Promise<MutationResponse> {
  return invoke('insert_row', { sessionId, database, schema, table, data, acknowledgedDangerous });
}

export async function updateRow(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  table: string,
  primaryKey: RowData,
  data: RowData,
  acknowledgedDangerous?: boolean
): Promise<MutationResponse> {
  return invoke('update_row', {
    sessionId,
    database,
    schema,
    table,
    primaryKey,
    data,
    acknowledgedDangerous,
  });
}

export async function deleteRow(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  table: string,
  primaryKey: RowData,
  acknowledgedDangerous?: boolean
): Promise<MutationResponse> {
  return invoke('delete_row', {
    sessionId,
    database,
    schema,
    table,
    primaryKey,
    acknowledgedDangerous,
  });
}

export async function supportsMutations(sessionId: string): Promise<boolean> {
  return invoke('supports_mutations', { sessionId });
}
