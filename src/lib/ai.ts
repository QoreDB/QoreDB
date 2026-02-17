// SPDX-License-Identifier: BUSL-1.1

/**
 * AI BYOK types and Tauri bindings.
 * Mirrors Rust types from src-tauri/src/ai/types.rs
 */
import { invoke } from '@tauri-apps/api/core';
import type { Namespace } from './tauri';

// ============================================
// TYPES
// ============================================

export type AiProvider = 'open_ai' | 'anthropic' | 'ollama';

export type AiAction = 'generate_query' | 'explain_result' | 'summarize_schema' | 'fix_error';

export interface AiConfig {
  provider: AiProvider;
  model?: string;
  base_url?: string;
  max_tokens?: number;
  temperature?: number;
}

export interface AiRequest {
  request_id: string;
  action: AiAction;
  prompt: string;
  session_id: string;
  namespace?: Namespace;
  connection_id?: string;
  config: AiConfig;
  original_query?: string;
  error_context?: string;
  result_context?: string;
}

export interface SafetyInfo {
  is_mutation: boolean;
  is_dangerous: boolean;
  warnings: string[];
}

export interface AiStreamChunk {
  request_id: string;
  delta: string;
  done: boolean;
  error?: string;
  generated_query?: string;
  safety_analysis?: SafetyInfo;
}

export interface AiResponse {
  request_id: string;
  content: string;
  generated_query?: string;
  safety_analysis?: SafetyInfo;
  provider_used: AiProvider;
  tokens_used?: number;
}

export interface AiProviderStatus {
  provider: AiProvider;
  has_key: boolean;
  model?: string;
  base_url?: string;
}

// ============================================
// PROVIDER DISPLAY INFO
// ============================================

export const AI_PROVIDERS: {
  id: AiProvider;
  label: string;
  defaultModel: string;
  requiresKey: boolean;
}[] = [
  { id: 'open_ai', label: 'OpenAI', defaultModel: 'gpt-4o', requiresKey: true },
  {
    id: 'anthropic',
    label: 'Anthropic',
    defaultModel: 'claude-sonnet-4-20250514',
    requiresKey: true,
  },
  { id: 'ollama', label: 'Ollama', defaultModel: 'llama3', requiresKey: false },
];

// ============================================
// TAURI COMMANDS
// ============================================

/** Start streaming AI query generation (response comes via events) */
export async function aiGenerateQuery(request: AiRequest): Promise<void> {
  return invoke('ai_generate_query', { request });
}

/** Explain a query result (non-streaming) */
export async function aiExplainResult(
  sessionId: string,
  query: string,
  resultSummary: string,
  config: AiConfig,
  namespace?: Namespace
): Promise<AiResponse> {
  return invoke('ai_explain_result', { sessionId, query, resultSummary, config, namespace });
}

/** Summarize the schema (non-streaming) */
export async function aiSummarizeSchema(
  sessionId: string,
  config: AiConfig,
  namespace?: Namespace
): Promise<AiResponse> {
  return invoke('ai_summarize_schema', { sessionId, config, namespace });
}

/** Start streaming AI error fix (response comes via events) */
export async function aiFixError(request: AiRequest): Promise<void> {
  return invoke('ai_fix_error', { request });
}

/** Save an API key for a provider */
export async function aiSaveApiKey(provider: AiProvider, key: string): Promise<void> {
  return invoke('ai_save_api_key', { provider, key });
}

/** Delete an API key for a provider */
export async function aiDeleteApiKey(provider: AiProvider): Promise<void> {
  return invoke('ai_delete_api_key', { provider });
}

/** Get status of all configured providers */
export async function aiGetProviderStatus(): Promise<AiProviderStatus[]> {
  return invoke('ai_get_provider_status');
}

// ============================================
// EVENT HELPERS
// ============================================

/** Returns the Tauri event name for streaming AI responses */
export function aiStreamEvent(requestId: string): string {
  return `ai_stream:${requestId}`;
}
