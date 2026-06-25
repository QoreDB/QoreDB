// SPDX-License-Identifier: BUSL-1.1

import { invoke } from '@/lib/transport';
import { createStreamChannel, type QueryResult, type QueryStreamHandlers } from '../tauri';

export interface FederationSource {
  alias: string;
  session_id: string;
  driver: string;
  display_name: string;
}

export interface SourceFetchInfo {
  alias: string;
  table: string;
  row_count: number;
  fetch_time_ms: number;
  row_limit_hit: boolean;
}

export interface FederationMeta {
  source_results: SourceFetchInfo[];
  duckdb_time_ms: number;
  total_time_ms: number;
  warnings: string[];
}

export interface FederationQueryResponse {
  success: boolean;
  result?: QueryResult;
  error?: string;
  query_id?: string;
  federation?: FederationMeta;
}

export interface FederationQueryOptions {
  timeoutMs?: number;
  stream?: boolean;
  queryId?: string;
  rowLimitPerSource?: number;
  streamHandlers?: QueryStreamHandlers;
}

export async function executeFederationQuery(
  query: string,
  aliasMap: Record<string, string>,
  options?: FederationQueryOptions
): Promise<FederationQueryResponse> {
  const channel = createStreamChannel(options?.streamHandlers ?? {});
  return invoke('execute_federation_query', {
    query,
    aliasMap,
    options: options
      ? {
          timeout_ms: options.timeoutMs,
          stream: options.stream,
          query_id: options.queryId,
          row_limit_per_source: options.rowLimitPerSource,
        }
      : undefined,
    onStream: channel,
  });
}

export async function listFederationSources(): Promise<FederationSource[]> {
  return invoke('list_federation_sources');
}

/**
 * Quick regex-based detection of whether a query contains cross-database
 * federation syntax (3-part identifiers where the first part is a known alias).
 *
 * This is a fast pre-check; the backend does full AST-based validation.
 */
export function isFederationQuery(query: string, knownAliases: Set<string>): boolean {
  if (knownAliases.size === 0) return false;

  // Match 3-part identifiers: word.word.word
  // Excludes common false positives in strings/comments
  const pattern = /\b(\w+)\.(\w+)\.(\w+)\b/g;
  let match: RegExpExecArray | null = pattern.exec(query);
  while (match !== null) {
    const candidate = match[1].toLowerCase();
    if (knownAliases.has(candidate)) {
      return true;
    }
    match = pattern.exec(query);
  }
  return false;
}

export function buildAliasSet(sources: FederationSource[]): Set<string> {
  return new Set(sources.map(s => s.alias));
}

export function buildAliasMap(sources: FederationSource[]): Record<string, string> {
  return Object.fromEntries(sources.map(s => [s.alias, s.session_id]));
}
