// SPDX-License-Identifier: Apache-2.0

import { getWorkspaceState } from './stores/workspaceStore';
import type { Namespace } from './tauri';

export interface TableVisitInsight {
  key: string;
  connectionId?: string;
  database: string;
  schema?: string;
  tableName: string;
  lastVisitedAt: number;
}

interface RecordTableVisitParams {
  connectionId?: string;
  namespace: Namespace;
  tableName: string;
}

const STORAGE_KEY_PREFIX = 'qoredb_table_insights';
const MAX_ENTRIES = 500;

let inMemoryInsights: TableVisitInsight[] = [];

function getStorageKey(): string {
  const { projectId } = getWorkspaceState();
  return projectId === 'default' ? STORAGE_KEY_PREFIX : `${STORAGE_KEY_PREFIX}_${projectId}`;
}

function buildInsightKey(
  connectionId: string | undefined,
  namespace: Namespace,
  tableName: string
): string {
  return [connectionId ?? '', namespace.database, namespace.schema ?? '', tableName].join('::');
}

function normalizeInsights(value: unknown): TableVisitInsight[] {
  if (!Array.isArray(value)) return [];

  const normalized: TableVisitInsight[] = [];
  const seen = new Set<string>();

  for (const entry of value) {
    if (!entry || typeof entry !== 'object') continue;

    const candidate = entry as Partial<TableVisitInsight>;
    const tableName = typeof candidate.tableName === 'string' ? candidate.tableName.trim() : '';
    const database = typeof candidate.database === 'string' ? candidate.database.trim() : '';

    if (!tableName || !database) continue;

    const schema =
      typeof candidate.schema === 'string' && candidate.schema.trim()
        ? candidate.schema.trim()
        : undefined;
    const connectionId =
      typeof candidate.connectionId === 'string' && candidate.connectionId.trim()
        ? candidate.connectionId.trim()
        : undefined;

    const key = buildInsightKey(connectionId, { database, schema }, tableName);
    if (seen.has(key)) continue;
    seen.add(key);

    const lastVisitedAt =
      typeof candidate.lastVisitedAt === 'number' && Number.isFinite(candidate.lastVisitedAt)
        ? candidate.lastVisitedAt
        : 0;
    normalized.push({
      key,
      connectionId,
      database,
      schema,
      tableName,
      lastVisitedAt,
    });
  }

  return normalized;
}

function getStoredInsights(): TableVisitInsight[] {
  if (typeof window === 'undefined') {
    return inMemoryInsights;
  }

  try {
    const raw = window.localStorage.getItem(getStorageKey());
    if (!raw) return [];
    return normalizeInsights(JSON.parse(raw));
  } catch {
    return inMemoryInsights;
  }
}

function persistInsights(entries: TableVisitInsight[]): void {
  const trimmed = entries
    .slice()
    .sort((a, b) => b.lastVisitedAt - a.lastVisitedAt)
    .slice(0, MAX_ENTRIES);

  inMemoryInsights = trimmed;

  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(getStorageKey(), JSON.stringify(trimmed));
  } catch {
    // Ignore storage failures and keep the in-memory fallback.
  }
}

function matchesNamespace(
  insight: TableVisitInsight,
  namespace: Namespace,
  connectionId?: string
): boolean {
  if (insight.database !== namespace.database) return false;
  if ((insight.schema ?? '') !== (namespace.schema ?? '')) return false;
  if (connectionId !== undefined) {
    return insight.connectionId === connectionId;
  }
  return true;
}

export function recordTableVisit({
  connectionId,
  namespace,
  tableName,
}: RecordTableVisitParams): TableVisitInsight {
  const insights = getStoredInsights();
  const now = Date.now();
  const key = buildInsightKey(connectionId, namespace, tableName);
  const existingIndex = insights.findIndex(entry => entry.key === key);

  if (existingIndex >= 0) {
    const existing = insights[existingIndex];
    const updated: TableVisitInsight = {
      ...existing,
      lastVisitedAt: now,
    };
    insights[existingIndex] = updated;
    persistInsights(insights);
    return updated;
  }

  const created: TableVisitInsight = {
    key,
    connectionId,
    database: namespace.database,
    schema: namespace.schema,
    tableName,
    lastVisitedAt: now,
  };

  insights.unshift(created);
  persistInsights(insights);
  return created;
}

export function getNamespaceTableVisits(
  namespace: Namespace,
  connectionId?: string,
  limit = 10
): TableVisitInsight[] {
  return getStoredInsights()
    .filter(entry => matchesNamespace(entry, namespace, connectionId))
    .sort((a, b) => b.lastVisitedAt - a.lastVisitedAt || a.tableName.localeCompare(b.tableName))
    .slice(0, limit);
}

export function removeTableVisit(
  namespace: Namespace,
  tableName: string,
  connectionId?: string
): void {
  const next = getStoredInsights().filter(
    entry => !matchesNamespace(entry, namespace, connectionId) || entry.tableName !== tableName
  );
  persistInsights(next);
}
