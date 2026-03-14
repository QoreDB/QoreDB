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

export type AiProvider =
  | 'open_ai'
  | 'anthropic'
  | 'mistral_ai'
  | 'google_gemini'
  | 'deep_seek'
  | 'ollama';

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

// ============================================
// PROVIDER DISPLAY INFO
// ============================================

export interface AiProviderInfo {
  id: AiProvider;
  label: string;
  models: AiModelInfo[];
  requiresKey: boolean;
}

export const AI_PROVIDERS: AiProviderInfo[] = [
  {
    id: 'open_ai',
    label: 'OpenAI',
    requiresKey: true,
    models: [
      { id: 'gpt-4.1', label: 'GPT-4.1' },
      { id: 'gpt-4.1-mini', label: 'GPT-4.1 Mini' },
      { id: 'gpt-4.1-nano', label: 'GPT-4.1 Nano' },
      { id: 'o4-mini', label: 'o4-mini' },
      { id: 'o3-mini', label: 'o3-mini' },
    ],
  },
  {
    id: 'anthropic',
    label: 'Anthropic',
    requiresKey: true,
    models: [
      { id: 'claude-sonnet-4-6', label: 'Claude Sonnet 4.6' },
      { id: 'claude-sonnet-4-20250514', label: 'Claude Sonnet 4' },
      { id: 'claude-haiku-4-5-20251001', label: 'Claude Haiku 4.5' },
    ],
  },
  {
    id: 'mistral_ai',
    label: 'Mistral AI',
    requiresKey: true,
    models: [
      { id: 'mistral-large-latest', label: 'Mistral Large' },
      { id: 'mistral-medium-latest', label: 'Mistral Medium' },
      { id: 'mistral-small-latest', label: 'Mistral Small' },
      { id: 'codestral-latest', label: 'Codestral' },
      { id: 'pixtral-large-latest', label: 'Pixtral Large' },
    ],
  },
  {
    id: 'google_gemini',
    label: 'Google Gemini',
    requiresKey: true,
    models: [
      { id: 'gemini-2.5-pro-preview-05-06', label: 'Gemini 2.5 Pro' },
      { id: 'gemini-2.5-flash-preview-05-20', label: 'Gemini 2.5 Flash' },
      { id: 'gemini-2.0-flash', label: 'Gemini 2.0 Flash' },
    ],
  },
  {
    id: 'deep_seek',
    label: 'DeepSeek',
    requiresKey: true,
    models: [
      { id: 'deepseek-chat', label: 'DeepSeek V3' },
      { id: 'deepseek-reasoner', label: 'DeepSeek R1' },
    ],
  },
  {
    id: 'ollama',
    label: 'Ollama',
    requiresKey: false,
    models: [
      { id: 'llama3.3', label: 'Llama 3.3' },
      { id: 'llama3.1', label: 'Llama 3.1' },
      { id: 'qwen2.5-coder', label: 'Qwen 2.5 Coder' },
      { id: 'deepseek-r1', label: 'DeepSeek R1' },
      { id: 'codellama', label: 'Code Llama' },
      { id: 'mistral', label: 'Mistral' },
    ],
  },
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
