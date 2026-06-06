// SPDX-License-Identifier: Apache-2.0

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import type { QueryStreamHandlers } from './tauri/query';
import type { Namespace, QueryResult } from './tauri/types';

interface WebGlobals {
  __QORE_WEB__?: boolean;
  __QORE_TOKEN__?: string;
}

function globals(): WebGlobals {
  return typeof window === 'undefined' ? {} : (window as unknown as WebGlobals);
}

export const isWeb = globals().__QORE_WEB__ === true;

function authHeaders(): Record<string, string> {
  return {
    'Content-Type': 'application/json',
    Authorization: `Bearer ${globals().__QORE_TOKEN__ ?? ''}`,
  };
}

export async function invoke<T = unknown>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  if (!isWeb) {
    return tauriInvoke<T>(command, args);
  }
  const res = await fetch('/api/invoke', {
    method: 'POST',
    headers: authHeaders(),
    body: JSON.stringify({ command, args: args ?? {} }),
  });
  if (!res.ok) {
    throw new Error(await errorMessage(res, command));
  }
  return (await res.json()) as T;
}

export interface ExecuteQueryResult {
  success: boolean;
  result?: QueryResult;
  error?: string;
  query_id?: string;
  truncated?: boolean;
  truncated_total?: number;
}

export async function webExecuteQuery(
  sessionId: string,
  query: string,
  options?: {
    acknowledgedDangerous?: boolean;
    timeoutMs?: number;
    namespace?: Namespace;
    streamHandlers?: QueryStreamHandlers;
    bypassLimits?: boolean;
  }
): Promise<ExecuteQueryResult> {
  const handlers = options?.streamHandlers ?? {};
  const res = await fetch('/api/stream/execute_query', {
    method: 'POST',
    headers: authHeaders(),
    body: JSON.stringify({
      sessionId,
      query,
      namespace: options?.namespace,
      acknowledgedDangerous: options?.acknowledgedDangerous ?? false,
      timeoutMs: options?.timeoutMs,
      bypassLimits: options?.bypassLimits ?? false,
    }),
  });
  if (!res.ok || !res.body) {
    return { success: false, error: await errorMessage(res, 'execute_query') };
  }

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let failure: string | undefined;

  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    let sep = buffer.indexOf('\n\n');
    while (sep !== -1) {
      dispatchEvent(buffer.slice(0, sep), handlers, msg => {
        failure = msg;
      });
      buffer = buffer.slice(sep + 2);
      sep = buffer.indexOf('\n\n');
    }
  }

  return failure ? { success: false, error: failure } : { success: true };
}

function dispatchEvent(
  raw: string,
  handlers: QueryStreamHandlers,
  onFailure: (message: string) => void
): void {
  let event = 'message';
  let data = '';
  for (const line of raw.split('\n')) {
    if (line.startsWith('event:')) event = line.slice(6).trim();
    else if (line.startsWith('data:')) data += line.slice(5).trim();
  }
  if (!data) return;

  let parsed: unknown;
  try {
    parsed = JSON.parse(data);
  } catch {
    return;
  }

  switch (event) {
    case 'columns':
      handlers.onColumns?.(parsed as never);
      break;
    case 'row':
      handlers.onRow?.(parsed as never);
      break;
    case 'rows':
      handlers.onRowBatch?.(parsed as never);
      break;
    case 'done':
      handlers.onDone?.(parsed as number);
      break;
    case 'error':
      handlers.onError?.(parsed as string);
      onFailure(parsed as string);
      break;
  }
}

async function errorMessage(res: Response, command: string): Promise<string> {
  try {
    const body = await res.json();
    if (body?.error) return body.error as string;
  } catch {
    // fall through
  }
  return `${command} failed (${res.status})`;
}
