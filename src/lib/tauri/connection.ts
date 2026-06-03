// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@tauri-apps/api/core';
import type {
  ConnectionConfig,
  ConnectionResponse,
  SafetyPolicy,
  SafetyPolicyResponse,
  SessionListItem,
} from './types';

// ============================================
// CONNECTION URL PARSING
// ============================================

export type ParseErrorCode =
  | 'invalid_url'
  | 'unsupported_scheme'
  | 'missing_host'
  | 'invalid_port'
  | 'invalid_utf8';

export interface PartialConnectionConfig {
  driver?: string;
  host?: string;
  port?: number;
  username?: string;
  password?: string;
  database?: string;
  ssl?: boolean;
  options: Record<string, string>;
}

export interface ParseConnectionUrlResponse {
  success: boolean;
  config?: PartialConnectionConfig;
  error?: string;
  error_code?: ParseErrorCode;
}

export async function parseConnectionUrl(url: string): Promise<ParseConnectionUrlResponse> {
  return invoke('parse_url', { url });
}

export async function getSupportedUrlSchemes(): Promise<string[]> {
  return invoke('get_supported_url_schemes');
}

// ============================================
// CONNECTION COMMANDS
// ============================================

export async function testConnection(config: ConnectionConfig): Promise<ConnectionResponse> {
  return invoke('test_connection', { config });
}

export async function testSavedConnection(
  projectId: string,
  connectionId: string
): Promise<ConnectionResponse> {
  return invoke('test_saved_connection', { projectId, connectionId });
}

export async function connect(config: ConnectionConfig): Promise<ConnectionResponse> {
  return invoke('connect', { config });
}

export async function connectSavedConnection(
  projectId: string,
  connectionId: string
): Promise<ConnectionResponse> {
  return invoke('connect_saved_connection', { projectId, connectionId });
}

export async function disconnect(sessionId: string): Promise<ConnectionResponse> {
  return invoke('disconnect', { sessionId });
}

export async function listSessions(): Promise<SessionListItem[]> {
  return invoke('list_sessions');
}

export async function checkConnectionHealth(sessionId: string): Promise<string> {
  return invoke('check_connection_health', { sessionId });
}

export type ConnectionHealth = 'healthy' | 'unhealthy' | 'reconnecting';

export interface ConnectionHealthEvent {
  session_id: string;
  health: ConnectionHealth;
}

// ============================================
// POLICY COMMANDS
// ============================================

export async function getSafetyPolicy(): Promise<SafetyPolicyResponse> {
  return invoke('get_safety_policy');
}

export async function setSafetyPolicy(policy: SafetyPolicy): Promise<SafetyPolicyResponse> {
  return invoke('set_safety_policy', { policy });
}
