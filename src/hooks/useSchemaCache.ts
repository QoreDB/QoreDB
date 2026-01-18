/**
 * Schema Cache Hook
 *
 * Provides cached access to database schema information (namespaces, collections, table schemas).
 * Reduces redundant API calls and improves tree browsing performance.
 *
 * Cache is per-session and invalidated on DDL operations.
 */

import { useState, useCallback, useMemo } from 'react';
import {
  Namespace,
  Collection,
  TableSchema,
  listNamespaces,
  listCollections,
  describeTable,
} from '../lib/tauri';

// ============================================
// TYPES
// ============================================

interface NamespaceCache {
  namespaces: Namespace[];
  timestamp: number;
}

interface CollectionCache {
  collections: Collection[];
  timestamp: number;
}

interface TableSchemaCache {
  schema: TableSchema;
  timestamp: number;
}

interface SessionCache {
  namespaces: NamespaceCache | null;
  collections: Map<string, CollectionCache>;
  tableSchemas: Map<string, TableSchemaCache>;
}

// ============================================
// GLOBAL STORE (singleton per app)
// ============================================

const sessionCaches = new Map<string, SessionCache>();

// Cache TTL: 5 minutes (schema doesn't change often)
const CACHE_TTL_MS = 5 * 60 * 1000;

function getOrCreateSessionCache(sessionId: string): SessionCache {
  let cache = sessionCaches.get(sessionId);
  if (!cache) {
    cache = {
      namespaces: null,
      collections: new Map(),
      tableSchemas: new Map(),
    };
    sessionCaches.set(sessionId, cache);
  }
  return cache;
}

function isExpired(timestamp: number): boolean {
  return Date.now() - timestamp > CACHE_TTL_MS;
}

function getNamespaceKey(ns: Namespace): string {
  return `${ns.database}:${ns.schema || ''}`;
}

function getTableKey(ns: Namespace, tableName: string): string {
  return `${ns.database}:${ns.schema || ''}:${tableName}`;
}

// ============================================
// CACHE INVALIDATION (exported for DDL operations)
// ============================================

/**
 * Invalidate the entire cache for a session (used on disconnect)
 */
export function invalidateSessionCache(sessionId: string): void {
  sessionCaches.delete(sessionId);
}

/**
 * Invalidate namespace list cache (used after CREATE/DROP DATABASE/SCHEMA)
 */
export function invalidateNamespacesCache(sessionId: string): void {
  const cache = sessionCaches.get(sessionId);
  if (cache) {
    cache.namespaces = null;
  }
}

/**
 * Invalidate collections cache for a namespace (used after CREATE/DROP TABLE)
 */
export function invalidateCollectionsCache(sessionId: string, ns: Namespace): void {
  const cache = sessionCaches.get(sessionId);
  if (cache) {
    const key = getNamespaceKey(ns);
    cache.collections.delete(key);
  }
}

/**
 * Invalidate table schema cache (used after ALTER TABLE)
 */
export function invalidateTableSchemaCache(
  sessionId: string,
  ns: Namespace,
  tableName: string
): void {
  const cache = sessionCaches.get(sessionId);
  if (cache) {
    const key = getTableKey(ns, tableName);
    cache.tableSchemas.delete(key);
  }
}

/**
 * Force refresh all caches for a session (manual refresh button)
 */
export function forceRefreshCache(sessionId: string): void {
  const cache = sessionCaches.get(sessionId);
  if (cache) {
    cache.namespaces = null;
    cache.collections.clear();
    cache.tableSchemas.clear();
  }
}

// ============================================
// HOOK
// ============================================

interface UseSchemaCache {
  // Cached data fetchers
  getNamespaces: () => Promise<Namespace[]>;
  getCollections: (ns: Namespace) => Promise<Collection[]>;
  getTableSchema: (ns: Namespace, tableName: string) => Promise<TableSchema | null>;

  // Invalidation helpers (for use after DDL)
  invalidateNamespaces: () => void;
  invalidateCollections: (ns: Namespace) => void;
  invalidateTable: (ns: Namespace, tableName: string) => void;
  forceRefresh: () => void;

  // Loading states
  loading: boolean;
}

export function useSchemaCache(sessionId: string): UseSchemaCache {
  const [loading, setLoading] = useState(false);

  const cache = useMemo(() => getOrCreateSessionCache(sessionId), [sessionId]);

  const getNamespaces = useCallback(async (): Promise<Namespace[]> => {
    // Check cache first
    if (cache.namespaces && !isExpired(cache.namespaces.timestamp)) {
      return cache.namespaces.namespaces;
    }

    setLoading(true);
    try {
      const result = await listNamespaces(sessionId);
      if (result.success && result.namespaces) {
        cache.namespaces = {
          namespaces: result.namespaces,
          timestamp: Date.now(),
        };
        return result.namespaces;
      }
      return [];
    } finally {
      setLoading(false);
    }
  }, [sessionId, cache]);

  const getCollections = useCallback(
    async (ns: Namespace): Promise<Collection[]> => {
      const key = getNamespaceKey(ns);

      // Check cache first
      const cached = cache.collections.get(key);
      if (cached && !isExpired(cached.timestamp)) {
        return cached.collections;
      }

      setLoading(true);
      try {
        const result = await listCollections(sessionId, ns);
        if (result.success && result.data) {
          cache.collections.set(key, {
            collections: result.data.collections,
            timestamp: Date.now(),
          });
          return result.data.collections;
        }
        return [];
      } finally {
        setLoading(false);
      }
    },
    [sessionId, cache]
  );

  const getTableSchema = useCallback(
    async (ns: Namespace, tableName: string): Promise<TableSchema | null> => {
      const key = getTableKey(ns, tableName);

      // Check cache first
      const cached = cache.tableSchemas.get(key);
      if (cached && !isExpired(cached.timestamp)) {
        return cached.schema;
      }

      setLoading(true);
      try {
        const result = await describeTable(sessionId, ns, tableName);
        if (result.success && result.schema) {
          cache.tableSchemas.set(key, {
            schema: result.schema,
            timestamp: Date.now(),
          });
          return result.schema;
        }
        return null;
      } finally {
        setLoading(false);
      }
    },
    [sessionId, cache]
  );

  const invalidateNamespaces = useCallback(() => {
    invalidateNamespacesCache(sessionId);
  }, [sessionId]);

  const invalidateCollections = useCallback(
    (ns: Namespace) => {
      invalidateCollectionsCache(sessionId, ns);
    },
    [sessionId]
  );

  const invalidateTable = useCallback(
    (ns: Namespace, tableName: string) => {
      invalidateTableSchemaCache(sessionId, ns, tableName);
    },
    [sessionId]
  );

  const forceRefresh = useCallback(() => {
    forceRefreshCache(sessionId);
  }, [sessionId]);

  return useMemo(() => ({
    getNamespaces,
    getCollections,
    getTableSchema,
    invalidateNamespaces,
    invalidateCollections,
    invalidateTable,
    forceRefresh,
    loading,
  }), [
    getNamespaces,
    getCollections,
    getTableSchema,
    invalidateNamespaces,
    invalidateCollections,
    invalidateTable,
    forceRefresh,
    loading,
  ]);
}
