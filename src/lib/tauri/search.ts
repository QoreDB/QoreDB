// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@tauri-apps/api/core';
import type { Namespace, Value } from './types';

// ============================================
// FULL-TEXT SEARCH
// ============================================

export interface FulltextMatch {
  namespace: Namespace;
  table_name: string;
  column_name: string;
  value_preview: string;
  row_preview: [string, Value][];
}

export interface SearchFilter {
  column: string;
  value: string;
  caseSensitive?: boolean;
}

export interface FulltextSearchOptions {
  max_results_per_table?: number;
  max_total_results?: number;
  case_sensitive?: boolean;
  namespaces?: Namespace[];
  tables?: string[];
}

export interface SearchStats {
  native_fulltext_count: number;
  pattern_match_count: number;
  timeout_count: number;
  error_count: number;
}

export interface FulltextSearchResponse {
  success: boolean;
  matches: FulltextMatch[];
  total_matches: number;
  tables_searched: number;
  search_time_ms: number;
  error?: string;
  truncated: boolean;
  stats: SearchStats;
}

export async function fulltextSearch(
  sessionId: string,
  searchTerm: string,
  options?: FulltextSearchOptions
): Promise<FulltextSearchResponse> {
  return invoke('fulltext_search', { sessionId, searchTerm, options });
}
