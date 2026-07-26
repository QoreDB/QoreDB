// SPDX-License-Identifier: BUSL-1.1

// Bindings for the Database Agent chat (agent_* / chat_* Tauri commands).
// Types mirror src-tauri/src/ai/agent/{types,orchestrator,store}.rs.

import { invoke } from '@/lib/transport';
import type { AiConfig, AiError } from './ai';

export interface ToolCall {
  id: string;
  name: string;
  input: unknown;
  thought_signature?: string;
}

export interface ToolResult {
  id: string;
  content: string;
  is_error: boolean;
}

export interface AgentUsage {
  input_tokens?: number;
  output_tokens?: number;
  cache_read_tokens?: number;
  cache_creation_tokens?: number;
}

export interface AgentMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
  tool_calls?: ToolCall[];
  tool_results?: ToolResult[];
}

export interface AgentChatRequest {
  request_id: string;
  session_id: string;
  connection_id?: string;
  prompt: string;
  history: AgentMessage[];
  config: AiConfig;
  max_iterations?: number;
}

export type AgentEvent =
  | { type: 'text_delta'; text: string }
  | { type: 'text_reset' }
  | {
      type: 'tool_call_started';
      call_id: string;
      name: string;
      input: unknown;
      thought_signature?: string;
    }
  | {
      type: 'tool_result';
      call_id: string;
      name: string;
      content: string;
      is_error: boolean;
    }
  | {
      type: 'permission_request';
      permission_id: string;
      call_id: string;
      name: string;
      input: unknown;
      reason: string;
      can_remember: boolean;
    }
  | {
      type: 'done';
      text: string;
      tokens_used?: number;
      usage?: AgentUsage;
      iterations: number;
    }
  | { type: 'error'; error: AiError };

export interface ToolStepSummary {
  name: string;
  summary: string;
  is_error: boolean;
}

export interface StoredMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
  tool_steps?: ToolStepSummary[];
  usage?: AgentUsage;
}

export interface Conversation {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  messages: StoredMessage[];
  scope: string[];
}

export interface ConversationMeta {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  message_count: number;
}

export function agentStreamEvent(requestId: string): string {
  return `agent_stream:${requestId}`;
}

export async function agentSendMessage(request: AgentChatRequest): Promise<void> {
  return invoke('agent_send_message', { request });
}

export async function agentRespondPermission(
  permissionId: string,
  approved: boolean,
  remember: boolean
): Promise<void> {
  return invoke('agent_respond_permission', {
    permissionId,
    approved,
    remember,
  });
}

export async function agentCancel(requestId: string): Promise<void> {
  return invoke('agent_cancel', { requestId });
}

export async function chatListConversations(): Promise<ConversationMeta[]> {
  return invoke('chat_list_conversations');
}

export async function chatLoadConversation(id: string): Promise<Conversation> {
  return invoke('chat_load_conversation', { id });
}

export async function chatSaveConversation(conversation: Conversation): Promise<Conversation> {
  return invoke('chat_save_conversation', { conversation });
}

export async function chatRenameConversation(id: string, title: string): Promise<Conversation> {
  return invoke('chat_rename_conversation', { id, title });
}

export async function chatDeleteConversation(id: string): Promise<void> {
  return invoke('chat_delete_conversation', { id });
}

export async function chatGenerateTitle(firstMessage: string, config: AiConfig): Promise<string> {
  return invoke('chat_generate_title', { firstMessage, config });
}
