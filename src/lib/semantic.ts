// SPDX-License-Identifier: BUSL-1.1

import { invoke } from '@/lib/transport';

export const DEFAULT_SEMANTIC_MODEL = 'nomic-embed-text';

export interface SemanticConfig {
  enabled: boolean;
  base_url?: string | null;
  model: string;
}

export interface SemanticStatus {
  enabled: boolean;
  model: string;
  base_url: string;
  ollama_running: boolean;
  model_available: boolean;
  index?: {
    objects: number;
    building: boolean;
  };
}

export interface IndexSummary {
  total: number;
  embedded: number;
  deleted: number;
  duration_ms: number;
}

export type SemanticSearchState =
  | 'ready'
  | 'disabled'
  | 'ollama_missing'
  | 'model_missing'
  | 'index_empty'
  | 'building';

export interface SemanticHit {
  object_id: string;
  kind: 'table' | 'column';
  database: string;
  schema: string | null;
  table: string;
  column: string | null;
  document: string;
  sensitive: boolean;
  score: number;
}

export interface SemanticSearchResponse {
  status: SemanticSearchState;
  results: SemanticHit[];
  error?: string;
}

export async function semanticStatus(sessionId?: string): Promise<SemanticStatus> {
  return invoke('semantic_status', { sessionId: sessionId ?? null });
}

export async function semanticSetConfig(config: SemanticConfig): Promise<SemanticStatus> {
  return invoke('semantic_set_config', { config });
}

export async function semanticReindex(sessionId: string): Promise<IndexSummary> {
  return invoke('semantic_reindex', { sessionId });
}

export async function semanticSearch(
  sessionId: string,
  query: string,
  limit?: number
): Promise<SemanticSearchResponse> {
  return invoke('semantic_search', { sessionId, query, limit: limit ?? null });
}
