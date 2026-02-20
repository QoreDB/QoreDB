// SPDX-License-Identifier: BUSL-1.1

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef, useState } from 'react';

import {
  type AiAction,
  type AiConfig,
  type AiRequest,
  type AiStreamChunk,
  aiFixError,
  aiGenerateQuery,
  aiStreamEvent,
  type SafetyInfo,
} from '@/lib/ai';
import type { Namespace } from '@/lib/tauri';

export interface AiAssistantState {
  loading: boolean;
  response: string;
  generatedQuery: string | null;
  safetyAnalysis: SafetyInfo | null;
  error: string | null;
}

interface UseAiAssistantOptions {
  sessionId: string | null;
  namespace?: Namespace | null;
  connectionId?: string;
}

export function useAiAssistant({ sessionId, namespace, connectionId }: UseAiAssistantOptions) {
  const [state, setState] = useState<AiAssistantState>({
    loading: false,
    response: '',
    generatedQuery: null,
    safetyAnalysis: null,
    error: null,
  });

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
    setState({
      loading: false,
      response: '',
      generatedQuery: null,
      safetyAnalysis: null,
      error: null,
    });
  }, []);

  const sendStreamingRequest = useCallback(
    async (action: AiAction, prompt: string, config: AiConfig, extra?: Partial<AiRequest>) => {
      if (!sessionId) {
        setState(prev => ({ ...prev, error: 'No active session' }));
        return;
      }

      // Cleanup previous request
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }

      const requestId = crypto.randomUUID();
      requestIdRef.current = requestId;

      setState({
        loading: true,
        response: '',
        generatedQuery: null,
        safetyAnalysis: null,
        error: null,
      });

      // Subscribe to streaming events
      const unlisten = await listen<AiStreamChunk>(aiStreamEvent(requestId), event => {
        const chunk = event.payload;

        // Ignore chunks from old requests
        if (chunk.request_id !== requestIdRef.current) return;

        setState(prev => {
          if (chunk.error) {
            return {
              ...prev,
              loading: false,
              error: chunk.error ?? null,
            };
          }

          if (chunk.done) {
            return {
              ...prev,
              loading: false,
              generatedQuery: chunk.generated_query ?? prev.generatedQuery,
              safetyAnalysis: chunk.safety_analysis ?? prev.safetyAnalysis,
            };
          }

          return {
            ...prev,
            response: prev.response + chunk.delta,
          };
        });
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
        ...extra,
      };

      try {
        if (action === 'fix_error') {
          await aiFixError(request);
        } else {
          await aiGenerateQuery(request);
        }
      } catch (err) {
        setState(prev => ({
          ...prev,
          loading: false,
          error:
            typeof err === 'string'
              ? err
              : err instanceof Error
                ? err.message
                : 'AI request failed',
        }));
      }
    },
    [sessionId, namespace, connectionId]
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
    ...state,
    generateQuery,
    fixError,
    reset,
  };
}
