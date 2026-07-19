// SPDX-License-Identifier: BUSL-1.1

/**
 * AI BYOK types and Tauri bindings.
 * Mirrors Rust types from src-tauri/src/ai/types.rs
 */
import { invoke } from '@/lib/transport';
import type { ColumnFilter, Namespace } from './tauri';

export type AiProvider =
  | 'open_ai'
  | 'anthropic'
  | 'mistral_ai'
  | 'google_gemini'
  | 'deep_seek'
  | 'ollama';

export type AiAction = 'generate_query' | 'explain_result' | 'summarize_schema' | 'fix_error';

export type AiRole = 'system' | 'user' | 'assistant';

export interface AiMessage {
  role: AiRole;
  content: string;
}

export interface EditorContext {
  current_query?: string;
  active_table?: string;
  last_error?: string;
  result_shape?: string;
}

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
  history?: AiMessage[];
  editor_context?: EditorContext;
  include_sample_rows?: boolean;
  original_query?: string;
  error_context?: string;
  result_context?: string;
}

export interface SafetyInfo {
  is_mutation: boolean;
  is_dangerous: boolean;
  warnings: string[];
}

export type AiErrorKind =
  | 'invalid_key'
  | 'rate_limited'
  | 'context_too_large'
  | 'network'
  | 'provider';

export interface AiError {
  kind: AiErrorKind;
  message: string;
  retry_after_secs?: number;
  provider?: string;
  http_status?: number;
  provider_code?: string;
  provider_error_type?: string;
  request_id?: string;
}

export interface AiStreamChunk {
  request_id: string;
  delta: string;
  done: boolean;
  error?: AiError;
  generated_query?: string;
  safety_analysis?: SafetyInfo;
  tokens_used?: number;
}

export interface AiResponse {
  request_id: string;
  content: string;
  generated_query?: string;
  safety_analysis?: SafetyInfo;
  provider_used: AiProvider;
  tokens_used?: number;
}

export interface AiModelInfo {
  id: string;
  label: string;
}

export interface AiProviderStatus {
  provider: AiProvider;
  has_key: boolean;
  default_model: string;
  models: AiModelInfo[];
  base_url?: string;
}

export interface AiProviderInfo {
  id: AiProvider;
  label: string;
  models: AiModelInfo[];
  requiresKey: boolean;
}

// Curated Qore AI catalogs (mirrors src-tauri/src/ai/types.rs). The backend
// intersects these with each provider's live model availability.
export const AI_PROVIDERS: AiProviderInfo[] = [
  {
    id: 'open_ai',
    label: 'OpenAI',
    requiresKey: true,
    models: [
      { id: 'gpt-5.6-terra', label: 'GPT-5.6 Terra · Balanced' },
      { id: 'gpt-5.6-sol', label: 'GPT-5.6 Sol · Best quality' },
      { id: 'gpt-5.6-luna', label: 'GPT-5.6 Luna · Fast' },
    ],
  },
  {
    id: 'anthropic',
    label: 'Anthropic',
    requiresKey: true,
    models: [
      { id: 'claude-sonnet-5', label: 'Claude Sonnet 5' },
      { id: 'claude-opus-4-8', label: 'Claude Opus 4.8' },
      { id: 'claude-haiku-4-5', label: 'Claude Haiku 4.5' },
    ],
  },
  {
    id: 'mistral_ai',
    label: 'Mistral AI',
    requiresKey: true,
    models: [
      { id: 'mistral-medium-latest', label: 'Mistral Medium' },
      { id: 'mistral-large-latest', label: 'Mistral Large' },
      { id: 'mistral-small-latest', label: 'Mistral Small' },
    ],
  },
  {
    id: 'google_gemini',
    label: 'Google Gemini',
    requiresKey: true,
    models: [
      { id: 'gemini-3.5-flash', label: 'Gemini 3.5 Flash' },
      { id: 'gemini-3.1-pro-preview', label: 'Gemini 3.1 Pro' },
      { id: 'gemini-3.1-flash-lite', label: 'Gemini 3.1 Flash Lite' },
    ],
  },
  {
    id: 'deep_seek',
    label: 'DeepSeek',
    requiresKey: true,
    models: [
      { id: 'deepseek-v4-flash', label: 'DeepSeek V4 Flash' },
      { id: 'deepseek-v4-pro', label: 'DeepSeek V4 Pro' },
    ],
  },
  {
    id: 'ollama',
    label: 'Ollama',
    requiresKey: false,
    models: [
      { id: 'qwen3', label: 'Qwen 3' },
      { id: 'llama3.3', label: 'Llama 3.3' },
      { id: 'deepseek-r1', label: 'DeepSeek R1' },
      { id: 'mistral', label: 'Mistral' },
    ],
  },
];

export async function aiGenerateQuery(request: AiRequest): Promise<void> {
  return invoke('ai_generate_query', { request });
}

export async function aiExplainResult(
  sessionId: string,
  query: string,
  resultSummary: string,
  config: AiConfig,
  namespace?: Namespace
): Promise<AiResponse> {
  return invoke('ai_explain_result', { sessionId, query, resultSummary, config, namespace });
}

export async function aiSummarizeSchema(
  sessionId: string,
  config: AiConfig,
  namespace?: Namespace
): Promise<AiResponse> {
  return invoke('ai_summarize_schema', { sessionId, config, namespace });
}

export async function aiFixError(request: AiRequest): Promise<void> {
  return invoke('ai_fix_error', { request });
}

export async function aiSaveApiKey(provider: AiProvider, key: string): Promise<void> {
  return invoke('ai_save_api_key', { provider, key });
}

export async function aiDeleteApiKey(provider: AiProvider): Promise<void> {
  return invoke('ai_delete_api_key', { provider });
}

export async function aiGetProviderStatus(probeProvider?: AiProvider): Promise<AiProviderStatus[]> {
  return invoke('ai_get_provider_status', { probeProvider });
}

/** Live model list from the provider API (cached per session backend-side);
 *  falls back to the curated list when the endpoint or key is unavailable. */
export async function aiListModels(provider: AiProvider, baseUrl?: string): Promise<AiModelInfo[]> {
  return invoke('ai_list_models', { provider, baseUrl });
}

/**
 * Translate a natural-language filter into structured column filters applied by
 * the grid (non-streaming). `today` lets the model resolve relative dates.
 */
export async function aiGenerateFilters(
  sessionId: string,
  tableName: string,
  prompt: string,
  config: AiConfig,
  namespace?: Namespace
): Promise<ColumnFilter[]> {
  const today = new Date().toISOString().slice(0, 10);
  return invoke('ai_generate_filters', {
    sessionId,
    tableName,
    prompt,
    today,
    config,
    namespace,
  });
}

export function aiStreamEvent(requestId: string): string {
  return `ai_stream:${requestId}`;
}
