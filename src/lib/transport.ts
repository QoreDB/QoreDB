// SPDX-License-Identifier: Apache-2.0

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import {
  type Event as TauriEvent,
  listen as tauriListen,
  type UnlistenFn,
} from '@tauri-apps/api/event';
import { openUrl as tauriOpenUrl } from '@tauri-apps/plugin-opener';
import type { QueryStreamHandlers } from './tauri/query';
import type { Namespace, QueryResult } from './tauri/types';

export type { UnlistenFn };

interface WebGlobals {
  __QORE_WEB__?: boolean;
}

function globals(): WebGlobals {
  return typeof window === 'undefined' ? {} : (window as unknown as WebGlobals);
}

export const isWeb = globals().__QORE_WEB__ === true;

const TOKEN_STORAGE_KEY = 'qore_auth_token';

function readStoredToken(): string {
  if (typeof window === 'undefined') return '';
  return window.sessionStorage?.getItem(TOKEN_STORAGE_KEY) ?? '';
}

let authToken = readStoredToken();

export function setAuthToken(token: string | null): void {
  authToken = token ?? '';
  if (typeof window === 'undefined') return;
  if (token) {
    window.sessionStorage?.setItem(TOKEN_STORAGE_KEY, token);
  } else {
    window.sessionStorage?.removeItem(TOKEN_STORAGE_KEY);
  }
}

export function isAuthenticated(): boolean {
  return authToken !== '';
}

export function listen<T = unknown>(
  event: string,
  handler: (event: TauriEvent<T>) => void
): Promise<UnlistenFn> {
  if (!isWeb) {
    return tauriListen<T>(event, handler);
  }
  return Promise.resolve(() => {});
}

export function openExternal(url: string): Promise<void> {
  if (isWeb) {
    window.open(url, '_blank', 'noopener,noreferrer');
    return Promise.resolve();
  }
  return tauriOpenUrl(url);
}

function authHeaders(): Record<string, string> {
  return {
    'Content-Type': 'application/json',
    Authorization: `Bearer ${authToken}`,
  };
}

export interface AuthStatus {
  setupRequired: boolean;
  ssoEnabled: boolean;
}

export interface LoginResult {
  token: string;
  email: string;
  isAdmin: boolean;
}

export async function webAuthStatus(): Promise<AuthStatus> {
  const res = await fetch('/api/auth/status');
  if (!res.ok) throw new Error(await errorMessage(res, 'auth/status'));
  return (await res.json()) as AuthStatus;
}

export async function webRegister(email: string, password: string): Promise<void> {
  const res = await fetch('/api/auth/register', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email, password }),
  });
  if (!res.ok) throw new Error(await errorMessage(res, 'auth/register'));
}

export async function webLogin(email: string, password: string): Promise<LoginResult> {
  const res = await fetch('/api/auth/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email, password }),
  });
  if (!res.ok) throw new Error(await errorMessage(res, 'auth/login'));
  const result = (await res.json()) as LoginResult;
  setAuthToken(result.token);
  return result;
}

export function webSsoStart(): void {
  if (typeof window !== 'undefined') {
    window.location.href = '/api/auth/oidc/start';
  }
}

/**
 * Reads the `?sso_token` / `?sso_error` the OIDC callback appended, stores the
 * token, and strips the params from the URL. Call once during web boot before
 * gating on `isAuthenticated()`.
 */
export function consumeSsoRedirect(): { error?: string } {
  if (typeof window === 'undefined') return {};
  const params = new URLSearchParams(window.location.search);
  const token = params.get('sso_token');
  const error = params.get('sso_error');
  if (!token && !error) return {};

  if (token) setAuthToken(token);
  params.delete('sso_token');
  params.delete('sso_error');
  const query = params.toString();
  const url = window.location.pathname + (query ? `?${query}` : '') + window.location.hash;
  window.history.replaceState({}, '', url);
  return error ? { error } : {};
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
