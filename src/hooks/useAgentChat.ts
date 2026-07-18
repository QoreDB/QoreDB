// SPDX-License-Identifier: BUSL-1.1

import { useCallback, useEffect, useRef, useState } from 'react';
import {
  type AgentEvent,
  type AgentMessage,
  agentCancel,
  agentRespondPermission,
  agentSendMessage,
  agentStreamEvent,
  type StoredMessage,
  type ToolStepSummary,
} from '@/lib/agent';
import type { AiConfig } from '@/lib/ai';
import { listen, type UnlistenFn } from '@/lib/transport';

export type AgentChatItem =
  | { kind: 'user'; id: string; content: string }
  | { kind: 'assistant'; id: string; content: string; streaming: boolean; error?: string | null }
  | {
      kind: 'tool';
      id: string;
      callId: string;
      name: string;
      input: unknown;
      result?: { content: string; isError: boolean };
    }
  | {
      kind: 'permission';
      id: string;
      permissionId: string;
      name: string;
      input: unknown;
      reason: string;
      canRemember: boolean;
      decision?: 'approved' | 'denied';
    };

interface UseAgentChatOptions {
  sessionId: string | null;
  connectionId?: string;
  /** Called when a run completes successfully (for auto-save). */
  onDone?: () => void;
}

export function useAgentChat({ sessionId, connectionId, onDone }: UseAgentChatOptions) {
  const [items, setItems] = useState<AgentChatItem[]>([]);
  const [loading, setLoading] = useState(false);

  const itemsRef = useRef<AgentChatItem[]>([]);
  itemsRef.current = items;
  const unlistenRef = useRef<UnlistenFn | null>(null);
  const requestIdRef = useRef<string | null>(null);
  const onDoneRef = useRef(onDone);
  onDoneRef.current = onDone;

  useEffect(() => {
    return () => {
      unlistenRef.current?.();
      unlistenRef.current = null;
    };
  }, []);

  const detach = useCallback(() => {
    unlistenRef.current?.();
    unlistenRef.current = null;
  }, []);

  const reset = useCallback(() => {
    detach();
    requestIdRef.current = null;
    setItems([]);
    setLoading(false);
  }, [detach]);

  /** Replaces the thread with persisted messages (conversation resume). */
  const loadMessages = useCallback(
    (messages: StoredMessage[]) => {
      detach();
      requestIdRef.current = null;
      setLoading(false);
      const restored: AgentChatItem[] = [];
      for (const message of messages) {
        if (message.role === 'user') {
          restored.push({ kind: 'user', id: crypto.randomUUID(), content: message.content });
        } else if (message.role === 'assistant') {
          for (const step of message.tool_steps ?? []) {
            restored.push({
              kind: 'tool',
              id: crypto.randomUUID(),
              callId: crypto.randomUUID(),
              name: step.name,
              input: undefined,
              result: { content: step.summary, isError: step.is_error },
            });
          }
          restored.push({
            kind: 'assistant',
            id: crypto.randomUUID(),
            content: message.content,
            streaming: false,
          });
        }
      }
      setItems(restored);
    },
    [detach]
  );

  const applyEvent = useCallback((event: AgentEvent) => {
    setItems(prev => {
      switch (event.type) {
        case 'text_delta': {
          const last = prev[prev.length - 1];
          if (last?.kind === 'assistant' && last.streaming) {
            return prev.map(item =>
              item.id === last.id && item.kind === 'assistant'
                ? { ...item, content: item.content + event.text }
                : item
            );
          }
          return [
            ...prev,
            {
              kind: 'assistant',
              id: crypto.randomUUID(),
              content: event.text,
              streaming: true,
            },
          ];
        }
        case 'tool_call_started':
          return [
            ...prev,
            {
              kind: 'tool',
              id: crypto.randomUUID(),
              callId: event.call_id,
              name: event.name,
              input: event.input,
            },
          ];
        case 'tool_result':
          return prev.map(item =>
            item.kind === 'tool' && item.callId === event.call_id && !item.result
              ? { ...item, result: { content: event.content, isError: event.is_error } }
              : item
          );
        case 'permission_request':
          return [
            ...prev,
            {
              kind: 'permission',
              id: crypto.randomUUID(),
              permissionId: event.permission_id,
              name: event.name,
              input: event.input,
              reason: event.reason,
              canRemember: event.can_remember,
            },
          ];
        case 'done': {
          const next = prev.map(item =>
            item.kind === 'assistant' ? { ...item, streaming: false } : item
          );
          // The final text may arrive without having been streamed (e.g.
          // fallback paths); make sure it is displayed.
          const last = next[next.length - 1];
          if (event.text && (last?.kind !== 'assistant' || last.content !== event.text)) {
            if (last?.kind === 'assistant' && last.content === '') {
              return next.map(item =>
                item.id === last.id && item.kind === 'assistant'
                  ? { ...item, content: event.text }
                  : item
              );
            }
            if (last?.kind !== 'assistant') {
              return [
                ...next,
                {
                  kind: 'assistant' as const,
                  id: crypto.randomUUID(),
                  content: event.text,
                  streaming: false,
                },
              ];
            }
          }
          return next;
        }
        case 'error': {
          const next = prev.map(item =>
            item.kind === 'assistant' ? { ...item, streaming: false } : item
          );
          return [
            ...next,
            {
              kind: 'assistant' as const,
              id: crypto.randomUUID(),
              content: '',
              streaming: false,
              error: event.error.message,
            },
          ];
        }
        default:
          return prev;
      }
    });
  }, []);

  const sendMessage = useCallback(
    async (prompt: string, config: AiConfig) => {
      if (!sessionId || loading) return;
      detach();

      const requestId = crypto.randomUUID();
      requestIdRef.current = requestId;

      // Option C: history carries text only, tool steps stay in the run.
      const history: AgentMessage[] = itemsRef.current.flatMap(item =>
        item.kind === 'user' || (item.kind === 'assistant' && item.content)
          ? [
              {
                role: item.kind === 'user' ? ('user' as const) : ('assistant' as const),
                content: item.content,
              },
            ]
          : []
      );

      setItems(prev => [...prev, { kind: 'user', id: crypto.randomUUID(), content: prompt }]);
      setLoading(true);

      const unlisten = await listen<AgentEvent>(agentStreamEvent(requestId), event => {
        if (requestIdRef.current !== requestId) return;
        const payload = event.payload;
        applyEvent(payload);
        if (payload.type === 'done' || payload.type === 'error') {
          setLoading(false);
          detach();
          if (payload.type === 'done') onDoneRef.current?.();
        }
      });
      unlistenRef.current = unlisten;

      try {
        await agentSendMessage({
          request_id: requestId,
          session_id: sessionId,
          connection_id: connectionId,
          prompt,
          history,
          config,
        });
      } catch (err) {
        applyEvent({
          type: 'error',
          error: {
            kind: 'provider',
            message:
              typeof err === 'string' ? err : err instanceof Error ? err.message : 'Agent error',
          },
        });
        setLoading(false);
        detach();
      }
    },
    [sessionId, connectionId, loading, detach, applyEvent]
  );

  const respondPermission = useCallback(
    async (permissionId: string, approved: boolean, remember: boolean) => {
      setItems(prev =>
        prev.map(item =>
          item.kind === 'permission' && item.permissionId === permissionId
            ? { ...item, decision: approved ? 'approved' : 'denied' }
            : item
        )
      );
      try {
        await agentRespondPermission(permissionId, approved, remember);
      } catch {
        // The run may already be gone (cancel/timeout); the decision chip stays.
      }
    },
    []
  );

  const cancel = useCallback(async () => {
    const requestId = requestIdRef.current;
    if (!requestId) return;
    try {
      await agentCancel(requestId);
    } catch {
      // Already finished.
    }
    setLoading(false);
  }, []);

  /** Serializes the thread for persistence (option C: summaries only). */
  const toStoredMessages = useCallback((): StoredMessage[] => {
    const out: StoredMessage[] = [];
    let pendingSteps: ToolStepSummary[] = [];
    for (const item of itemsRef.current) {
      if (item.kind === 'user') {
        out.push({ role: 'user', content: item.content });
      } else if (item.kind === 'tool') {
        pendingSteps.push({
          name: item.name,
          summary: summarizeToolStep(item),
          is_error: item.result?.isError ?? false,
        });
      } else if (item.kind === 'assistant' && (item.content || item.error)) {
        out.push({
          role: 'assistant',
          content: item.content || (item.error ?? ''),
          tool_steps: pendingSteps.length > 0 ? pendingSteps : undefined,
        });
        pendingSteps = [];
      }
    }
    return out;
  }, []);

  return {
    items,
    loading,
    sendMessage,
    respondPermission,
    cancel,
    reset,
    loadMessages,
    toStoredMessages,
  };
}

/** Short, results-free description of a tool step (option C invariant). */
function summarizeToolStep(item: Extract<AgentChatItem, { kind: 'tool' }>): string {
  const input = item.input as { query?: string } | undefined;
  const query = typeof input?.query === 'string' ? input.query : undefined;
  const base = query ? query.slice(0, 200) : item.name;
  if (item.result && !item.result.isError) {
    try {
      const parsed = JSON.parse(item.result.content) as { row_count?: number };
      if (typeof parsed.row_count === 'number') {
        return `${base} — ${parsed.row_count} rows`;
      }
    } catch {
      // Not a tabular payload.
    }
  }
  return base;
}
