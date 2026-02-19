// SPDX-License-Identifier: BUSL-1.1

/**
 * Cross-Database Federation — Frontend bindings and utilities.
 *
 * Provides type-safe Tauri invocations for federation queries
 * and client-side detection of federation syntax.
 */

import { invoke } from '@tauri-apps/api/core';
import type { QueryResult } from './tauri';

// ============================================
// TYPES
// ============================================

export interface FederationSource {
  alias: string;
  sessionId: string;
  driver: string;
  displayName: string;
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
}

// ============================================
// TAURI INVOCATIONS
// ============================================

/**
 * Executes a cross-database federation query.
 */
export async function executeFederationQuery(
  query: string,
  aliasMap: Record<string, string>,
  options?: FederationQueryOptions
): Promise<FederationQueryResponse> {
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
  });
}

/**
 * Lists all active connections available as federation sources.
 */
export async function listFederationSources(): Promise<FederationSource[]> {
  return invoke('list_federation_sources');
}

// ============================================
// CLIENT-SIDE DETECTION
// ============================================

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
  let match;
  while ((match = pattern.exec(query)) !== null) {
    const candidate = match[1].toLowerCase();
    if (knownAliases.has(candidate)) {
      return true;
    }
  }
  return false;
}

/**
 * Builds a Set of known aliases from federation sources.
 */
export function buildAliasSet(sources: FederationSource[]): Set<string> {
  return new Set(sources.map(s => s.alias));
}

/**
 * Builds the alias -> sessionId map from federation sources.
 */
export function buildAliasMap(sources: FederationSource[]): Record<string, string> {
  return Object.fromEntries(sources.map(s => [s.alias, s.sessionId]));
}
