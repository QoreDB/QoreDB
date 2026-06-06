// SPDX-License-Identifier: Apache-2.0

import { decode as msgpackDecode } from '@msgpack/msgpack';
import { Channel } from '@tauri-apps/api/core';
import { invoke, isWeb, webExecuteQuery } from '@/lib/transport';
import type { ForeignKey } from './schema-browse';
import type { CollectionList, ColumnInfo, Namespace, QueryResult, Row, Value } from './types';

// ============================================
// QUERY COMMANDS
// ============================================

/**
 * Handlers for streaming query events. When provided to `executeQuery` with
 * `stream: true`, the wrapper creates a Tauri Channel, receives MessagePack-
 * encoded events from the backend, and invokes the handlers directly. This
 * bypasses JSON entirely for large row batches.
 */
export interface QueryStreamHandlers {
  onColumns?: (cols: ColumnInfo[]) => void;
  onRow?: (row: Row) => void;
  onRowBatch?: (rows: Row[]) => void;
  onError?: (message: string) => void;
  onDone?: (affectedRows: number) => void;
}

interface StreamMsgEnvelope {
  t: 'c' | 'r' | 'rb' | 'e' | 'd';
  v: unknown;
}

export function createStreamChannel(handlers: QueryStreamHandlers): Channel<ArrayBuffer> {
  const channel = new Channel<ArrayBuffer>();
  channel.onmessage = payload => {
    // Backend sent InvokeResponseBody::Raw(bytes); payload is an ArrayBuffer
    // (or similar BufferSource depending on the runtime).
    let msg: StreamMsgEnvelope;
    try {
      msg = msgpackDecode(payload as ArrayBuffer) as StreamMsgEnvelope;
    } catch (err) {
      console.warn('failed to decode stream message', err);
      return;
    }
    switch (msg.t) {
      case 'c':
        handlers.onColumns?.(msg.v as ColumnInfo[]);
        break;
      case 'rb':
        handlers.onRowBatch?.(msg.v as Row[]);
        break;
      case 'r':
        handlers.onRow?.(msg.v as Row);
        break;
      case 'e':
        handlers.onError?.(msg.v as string);
        break;
      case 'd':
        handlers.onDone?.(msg.v as number);
        break;
    }
  };
  return channel;
}

export async function executeQuery(
  sessionId: string,
  query: string,
  options?: {
    acknowledgedDangerous?: boolean;
    timeoutMs?: number;
    stream?: boolean;
    queryId?: string;
    namespace?: Namespace;
    streamHandlers?: QueryStreamHandlers;
    bypassLimits?: boolean;
  }
): Promise<{
  success: boolean;
  result?: QueryResult;
  error?: string;
  query_id?: string;
  truncated?: boolean;
  truncated_total?: number;
}> {
  if (isWeb) {
    return webExecuteQuery(sessionId, query, options);
  }
  // The Rust command always expects an `on_stream` Channel — even for
  // non-streaming calls the arg is required. Create one with the caller's
  // handlers (empty object if none).
  const channel = createStreamChannel(options?.streamHandlers ?? {});
  return invoke('execute_query', {
    sessionId,
    query,
    namespace: options?.namespace,
    acknowledgedDangerous: options?.acknowledgedDangerous,
    queryId: options?.queryId,
    timeoutMs: options?.timeoutMs,
    stream: options?.stream,
    bypassLimits: options?.bypassLimits,
    onStream: channel,
  });
}

export async function listNamespaces(sessionId: string): Promise<{
  success: boolean;
  namespaces?: Namespace[];
  error?: string;
}> {
  return invoke('list_namespaces', { sessionId });
}

export async function listCollections(
  sessionId: string,
  namespace: Namespace,
  search?: string,
  page?: number,
  page_size?: number
): Promise<{
  success: boolean;
  data?: CollectionList;
  error?: string;
}> {
  return invoke('list_collections', { sessionId, namespace, search, page, page_size });
}

// ============================================
// PAGINATION TYPES AND QUERY
// ============================================

export type SortDirection = 'asc' | 'desc';

export type FilterOperator =
  | 'eq'
  | 'neq'
  | 'gt'
  | 'gte'
  | 'lt'
  | 'lte'
  | 'like'
  | 'is_null'
  | 'is_not_null'
  | 'regex'
  | 'text';

export interface FilterOptions {
  /** Regex flags string for `regex` operator (subset of `imxs`). */
  regex_flags?: string;
  /** Language tag for `text` operator (e.g. "english", "french"). */
  text_language?: string;
}

export interface ColumnFilter {
  column: string;
  operator: FilterOperator;
  value: Value;
  options?: FilterOptions;
}

export interface TableQueryOptions {
  page?: number;
  page_size?: number;
  sort_column?: string;
  sort_direction?: SortDirection;
  filters?: ColumnFilter[];
  search?: string;
}

export interface PaginatedQueryResult {
  result: QueryResult;
  total_rows: number;
  page: number;
  page_size: number;
}

export async function queryTable(
  sessionId: string,
  namespace: Namespace,
  table: string,
  options: TableQueryOptions = {},
  bypassCache: boolean = false
): Promise<{
  success: boolean;
  result?: PaginatedQueryResult;
  error?: string;
  /** True when the result was served from the query cache. */
  cached?: boolean;
  /** Age of the cached entry in milliseconds, when served from cache. */
  cached_age_ms?: number;
}> {
  return invoke('query_table', { sessionId, namespace, table, options, bypassCache });
}

// ============================================
// QUERY RESULT CACHE
// ============================================

export interface CacheConfig {
  enabled: boolean;
  ttlSecs: number;
  maxEntries: number;
}

export interface CacheStats {
  entries: number;
  hits: number;
  misses: number;
}

export async function getCacheConfig(): Promise<CacheConfig> {
  return invoke('get_cache_config');
}

export async function setCacheConfig(config: CacheConfig): Promise<CacheConfig> {
  return invoke('set_cache_config', { config });
}

export async function clearQueryCache(): Promise<void> {
  return invoke('clear_query_cache');
}

export async function getCacheStats(): Promise<CacheStats> {
  return invoke('get_cache_stats');
}

export async function peekForeignKey(
  sessionId: string,
  namespace: Namespace,
  foreignKey: ForeignKey,
  value: Value,
  limit: number = 3
): Promise<{
  success: boolean;
  result?: QueryResult;
  error?: string;
}> {
  return invoke('peek_foreign_key', { sessionId, namespace, foreignKey, value, limit });
}
