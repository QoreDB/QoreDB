// SPDX-License-Identifier: Apache-2.0

import { redactText } from '../redaction';
import { shouldStoreErrorLogs } from './diagnosticsSettings';

export interface ErrorLogEntry {
  id: string;
  timestamp: number;
  level: 'error' | 'warn' | 'info';
  source: string;
  message: string;
  details?: string;
  sessionId?: string;
}

const STORAGE_KEY = 'qoredb_error_logs';
const MAX_ENTRIES = 200;
const MAX_IN_MEMORY = 200;

let inMemoryLogs: ErrorLogEntry[] = [];

function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

export function getErrorLogs(): ErrorLogEntry[] {
  if (!shouldStoreErrorLogs()) {
    return inMemoryLogs;
  }
  try {
    const data = localStorage.getItem(STORAGE_KEY);
    if (!data) return [];
    return JSON.parse(data) as ErrorLogEntry[];
  } catch {
    return [];
  }
}

export function logError(
  source: string,
  message: string,
  details?: string,
  sessionId?: string,
  level: 'error' | 'warn' | 'info' = 'error'
): ErrorLogEntry {
  const logs = getErrorLogs();

  const entry: ErrorLogEntry = {
    id: generateId(),
    timestamp: Date.now(),
    level,
    source,
    message: redactText(message),
    details: details ? redactText(details) : undefined,
    sessionId,
  };

  logs.unshift(entry);

  if (shouldStoreErrorLogs()) {
    if (logs.length > MAX_ENTRIES) {
      logs.splice(MAX_ENTRIES);
    }
    localStorage.setItem(STORAGE_KEY, JSON.stringify(logs));
  } else {
    if (logs.length > MAX_IN_MEMORY) {
      logs.splice(MAX_IN_MEMORY);
    }
    inMemoryLogs = logs;
  }

  return entry;
}

export function logWarn(source: string, message: string, details?: string, sessionId?: string) {
  return logError(source, message, details, sessionId, 'warn');
}

export function logInfo(source: string, message: string, details?: string, sessionId?: string) {
  return logError(source, message, details, sessionId, 'info');
}

export function clearErrorLogs(): void {
  inMemoryLogs = [];
  localStorage.removeItem(STORAGE_KEY);
}

export function getLogsByLevel(level: 'error' | 'warn' | 'info'): ErrorLogEntry[] {
  return getErrorLogs().filter(e => e.level === level);
}

export function searchLogs(query: string): ErrorLogEntry[] {
  const lowerQuery = query.toLowerCase();
  return getErrorLogs().filter(
    e =>
      e.message.toLowerCase().includes(lowerQuery) ||
      e.source.toLowerCase().includes(lowerQuery) ||
      e.details?.toLowerCase().includes(lowerQuery)
  );
}
