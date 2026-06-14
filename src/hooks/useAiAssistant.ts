// SPDX-License-Identifier: BUSL-1.1

import { listen, type UnlistenFn } from '@/lib/transport';
import { useCallback, useEffect, useRef, useState } from 'react';

import {
  type AiAction,
  type AiConfig,
  type AiMessage,
  type AiRequest,
  type AiStreamChunk,
  aiFixError,
  aiGenerateQuery,
  aiStreamEvent,
  type EditorContext,
  type SafetyInfo,
} from '@/lib/ai';
import type { Namespace } from '@/lib/tauri';

export interface AiChatItem {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  generatedQuery?: string | null;
  safetyAnalysis?: SafetyInfo | null;
  error?: string | null;
  streaming?: boolean;
}

interface UseAiAssistantOptions {
  sessionId: string | null;
  namespace?: Namespace | null;
  connectionId?: string;
  getEditorContext?: () => EditorContext | undefined;
  includeSampleRows?: boolean;
}

export function useAiAssistant({
  sessionId,
  namespace,
  connectionId,
  getEditorContext,
  includeSampleRows,
}: UseAiAssistantOptions) {
  const [items, setItems] = useState<AiChatItem[]>([]);
  const [loading, setLoading] = useState(false);

  const itemsRef = useRef<AiChatItem[]>([]);
  itemsRef.current = items;

  const unlistenRef = useRef<UnlistenFn | null>(null);
  const requestIdRef = useRef<string | null>(null);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
    };
  }, []);

  const reset = useCallback(() => {
    if (unlistenRef.current) {
      unlistenRef.current();
      unlistenRef.current = null;
    }
    requestIdRef.current = null;
    setItems([]);
    setLoading(false);
  }, []);

  const sendStreamingRequest = useCallback(
    async (action: AiAction, prompt: string, config: AiConfig, extra?: Partial<AiRequest>) => {
      if (!sessionId) return;

      // Cleanup previous request
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }

      const requestId = crypto.randomUUID();
      requestIdRef.current = requestId;

      const history: AiMessage[] = itemsRef.current
        .filter(item => item.content && !item.error)
        .map(item => ({ role: item.role, content: item.content }));

      const assistantId = crypto.randomUUID();
      setItems(prev => [
        ...prev,
        { id: crypto.randomUUID(), role: 'user', content: prompt },
        { id: assistantId, role: 'assistant', content: '', streaming: true },
      ]);
      setLoading(true);

      const finalize = (patch: Partial<AiChatItem>) => {
        setItems(prev =>
          prev.map(item =>
            item.id === assistantId ? { ...item, streaming: false, ...patch } : item
          )
        );
        setLoading(false);
        if (unlistenRef.current) {
          unlistenRef.current();
          unlistenRef.current = null;
        }
      };

      // Subscribe to streaming events
      const unlisten = await listen<AiStreamChunk>(aiStreamEvent(requestId), event => {
        const chunk = event.payload;

        // Ignore chunks from old requests
        if (chunk.request_id !== requestIdRef.current) return;

        if (chunk.error) {
          finalize({ error: chunk.error });
          return;
        }

        if (chunk.done) {
          finalize({
            generatedQuery: chunk.generated_query ?? null,
            safetyAnalysis: chunk.safety_analysis ?? null,
          });
          return;
        }

        setItems(prev =>
          prev.map(item =>
            item.id === assistantId ? { ...item, content: item.content + chunk.delta } : item
          )
        );
      });

      unlistenRef.current = unlisten;

      const request: AiRequest = {
        request_id: requestId,
        action,
        prompt,
        session_id: sessionId,
        namespace: namespace ?? undefined,
        connection_id: connectionId,
        config,
        history,
        editor_context: getEditorContext?.(),
        include_sample_rows: includeSampleRows ?? false,
        ...extra,
      };

      try {
        if (action === 'fix_error') {
          await aiFixError(request);
        } else {
          await aiGenerateQuery(request);
        }
      } catch (err) {
        finalize({
          error:
            typeof err === 'string'
              ? err
              : err instanceof Error
                ? err.message
                : 'AI request failed',
        });
      }
    },
    [sessionId, namespace, connectionId, getEditorContext, includeSampleRows]
  );

  const generateQuery = useCallback(
    (prompt: string, config: AiConfig) => {
      return sendStreamingRequest('generate_query', prompt, config);
    },
    [sendStreamingRequest]
  );

  const fixError = useCallback(
    (prompt: string, config: AiConfig, originalQuery: string, errorContext: string) => {
      return sendStreamingRequest('fix_error', prompt, config, {
        original_query: originalQuery,
        error_context: errorContext,
      });
    },
    [sendStreamingRequest]
  );

  return {
    items,
    loading,
    generateQuery,
    fixError,
    reset,
  };
}
