// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@/lib/transport';
import type { ColumnInfo, Namespace, QueryResult } from './types';

export interface SnapshotMeta {
  id: string;
  name: string;
  description?: string;
  source: string;
  source_type: string;
  connection_name?: string;
  driver?: string;
  namespace?: Namespace;
  columns: ColumnInfo[];
  row_count: number;
  created_at: string;
  file_size: number;
}

export interface SaveSnapshotRequest {
  name: string;
  description?: string;
  source: string;
  source_type: string;
  connection_name?: string;
  driver?: string;
  namespace?: Namespace;
  result: QueryResult;
}

export async function saveSnapshot(
  request: SaveSnapshotRequest
): Promise<{ success: boolean; meta?: SnapshotMeta; error?: string }> {
  return invoke('save_snapshot', { request });
}

export async function listSnapshots(): Promise<{
  success: boolean;
  snapshots: SnapshotMeta[];
  error?: string;
}> {
  return invoke('list_snapshots');
}

export async function getSnapshot(
  snapshotId: string
): Promise<{ success: boolean; result?: QueryResult; meta?: SnapshotMeta; error?: string }> {
  return invoke('get_snapshot', { snapshotId });
}

export async function deleteSnapshot(
  snapshotId: string
): Promise<{ success: boolean; error?: string }> {
  return invoke('delete_snapshot', { snapshotId });
}

export async function renameSnapshot(
  snapshotId: string,
  newName: string
): Promise<{ success: boolean; meta?: SnapshotMeta; error?: string }> {
  return invoke('rename_snapshot', { snapshotId, newName });
}
