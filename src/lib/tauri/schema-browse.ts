// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@/lib/transport';
import type { Namespace, QueryResult, Value } from './types';

export interface CollationInfo {
  name: string;
  is_default: boolean;
}

export interface CharsetInfo {
  name: string;
  description: string;
  default_collation: string;
  collations: CollationInfo[];
}

export interface CreationOptions {
  charsets: CharsetInfo[];
}

export async function getCreationOptions(sessionId: string): Promise<{
  success: boolean;
  options?: CreationOptions;
  error?: string;
}> {
  return invoke('get_creation_options', { sessionId });
}

export async function createDatabase(
  sessionId: string,
  name: string,
  options?: Record<string, unknown>,
  acknowledgedDangerous?: boolean
): Promise<{
  success: boolean;
  error?: string;
}> {
  return invoke('create_database', { sessionId, name, options, acknowledgedDangerous });
}

export async function dropDatabase(
  sessionId: string,
  name: string,
  acknowledgedDangerous?: boolean
): Promise<{
  success: boolean;
  error?: string;
}> {
  return invoke('drop_database', { sessionId, name, acknowledgedDangerous });
}

export interface ForeignKey {
  column: string;
  referenced_table: string;
  referenced_column: string;
  referenced_schema?: string;
  referenced_database?: string;
  constraint_name?: string;
  is_virtual?: boolean;
}

export interface RelationFilter {
  foreignKey: ForeignKey;
  value: Value;
}

export interface TableIndex {
  name: string;
  columns: string[];
  is_unique: boolean;
  is_primary: boolean;
  /**
   * Engine-specific index type (e.g. "btree", "hash", "gin", "gist",
   * "fulltext", "text", "2dsphere"). Absent or `null` when not reported
   * by the driver.
   */
  index_type?: string | null;
}

export interface TableSchema {
  columns: TableColumn[];
  primary_key?: string[];
  foreign_keys: ForeignKey[];
  row_count_estimate?: number | null;
  indexes: TableIndex[];
}

export interface TableColumn {
  name: string;
  data_type: string;
  nullable: boolean;
  default_value?: string;
  is_primary_key: boolean;
  is_auto_increment?: boolean;
}

export type CancelSupport = 'none' | 'best_effort' | 'driver';

export interface DriverCapabilities {
  transactions: boolean;
  mutations: boolean;
  cancel: CancelSupport;
  supports_ssh: boolean;
  schema: boolean;
  streaming: boolean;
  explain: boolean;
  maintenance: boolean;
}

export interface DriverInfo {
  id: string;
  name: string;
  capabilities: DriverCapabilities;
}

export async function describeTable(
  sessionId: string,
  namespace: Namespace,
  table: string,
  connectionId?: string
): Promise<{
  success: boolean;
  schema?: TableSchema;
  error?: string;
}> {
  return invoke('describe_table', { sessionId, namespace, table, connectionId });
}

export async function previewTable(
  sessionId: string,
  namespace: Namespace,
  table: string,
  limit: number = 100,
  bypassCache: boolean = false
): Promise<{
  success: boolean;
  result?: QueryResult;
  error?: string;
}> {
  return invoke('preview_table', { sessionId, namespace, table, limit, bypassCache });
}

export interface VirtualRelation {
  id: string;
  source_database: string;
  source_schema?: string;
  source_table: string;
  source_column: string;
  referenced_table: string;
  referenced_column: string;
  referenced_schema?: string;
  referenced_database?: string;
  label?: string;
}

export async function listVirtualRelations(connectionId: string): Promise<{
  success: boolean;
  relations?: VirtualRelation[];
  error?: string;
}> {
  return invoke('list_virtual_relations', { connectionId });
}

export async function addVirtualRelation(
  connectionId: string,
  relation: VirtualRelation
): Promise<{ success: boolean; error?: string }> {
  return invoke('add_virtual_relation', { connectionId, relation });
}

export async function updateVirtualRelation(
  connectionId: string,
  relation: VirtualRelation
): Promise<{ success: boolean; error?: string }> {
  return invoke('update_virtual_relation', { connectionId, relation });
}

export async function deleteVirtualRelation(
  connectionId: string,
  relationId: string
): Promise<{ success: boolean; error?: string }> {
  return invoke('delete_virtual_relation', { connectionId, relationId });
}

export async function getDriverInfo(sessionId: string): Promise<{
  success: boolean;
  driver?: DriverInfo;
  error?: string;
}> {
  return invoke('get_driver_info', { sessionId });
}

export async function listDrivers(): Promise<{
  success: boolean;
  drivers: DriverInfo[];
  error?: string;
}> {
  return invoke('list_drivers');
}
