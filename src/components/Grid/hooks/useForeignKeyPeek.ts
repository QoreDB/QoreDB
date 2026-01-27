/**
 * Hook for foreign key peek functionality in DataGrid
 * Manages loading and caching of foreign key relationship previews
 */

import { useState, useCallback, useRef, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Value,
  Namespace,
  TableSchema,
  ForeignKey,
  QueryResult,
  peekForeignKey,
} from '@/lib/tauri';

export interface PeekState {
  status: 'idle' | 'loading' | 'ready' | 'error';
  result?: QueryResult;
  error?: string;
}

export interface UseForeignKeyPeekProps {
  sessionId?: string;
  namespace?: Namespace;
  tableSchema?: TableSchema | null;
}

export interface UseForeignKeyPeekReturn {
  peekCache: Map<string, PeekState>;
  foreignKeyMap: Map<string, ForeignKey[]>;
  buildPeekKey: (fk: ForeignKey, value: Value) => string;
  ensurePeekLoaded: (fk: ForeignKey, value: Value) => Promise<void>;
  resolveReferencedNamespace: (fk: ForeignKey) => Namespace | null;
  getRelationLabel: (fk: ForeignKey) => string;
}

export const MAX_PEEK_ROWS = 3;
export const MAX_PEEK_COLUMNS = 6;
const PEEK_QUERY_LIMIT = 6;

/**
 * Serializes a value for use as a cache key
 */
function serializePeekValue(value: Value): string {
  if (value === null) return 'null';
  if (typeof value === 'object') {
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  }
  return String(value);
}

/**
 * Hook for managing foreign key peek tooltips
 */
export function useForeignKeyPeek({
  sessionId,
  namespace,
  tableSchema,
}: UseForeignKeyPeekProps): UseForeignKeyPeekReturn {
  const { t } = useTranslation();
  const [peekCache, setPeekCache] = useState<Map<string, PeekState>>(new Map());
  const peekRequests = useRef(new Set<string>());

  // Build a map of column names to their foreign keys
  const foreignKeyMap = useMemo(() => {
    const map = new Map<string, ForeignKey[]>();
    if (!tableSchema?.foreign_keys?.length) return map;
    tableSchema.foreign_keys.forEach(fk => {
      if (!fk?.column) return;
      const entries = map.get(fk.column) ?? [];
      entries.push(fk);
      map.set(fk.column, entries);
    });
    return map;
  }, [tableSchema]);

  // Update the peek cache with a new state
  const updatePeekCache = useCallback((key: string, next: PeekState) => {
    setPeekCache(prev => {
      const updated = new Map(prev);
      updated.set(key, next);
      return updated;
    });
  }, []);

  // Resolve the namespace of a referenced table
  const resolveReferencedNamespace = useCallback(
    (foreignKey: ForeignKey): Namespace | null => {
      if (!namespace) return null;
      const database = foreignKey.referenced_database ?? namespace.database;
      const schema = foreignKey.referenced_schema ?? namespace.schema;
      return { database, schema };
    },
    [namespace]
  );

  // Get a display label for a foreign key relation
  const getRelationLabel = useCallback((foreignKey: ForeignKey): string => {
    if (foreignKey.referenced_database) {
      return `${foreignKey.referenced_database}.${foreignKey.referenced_table}`;
    }
    if (foreignKey.referenced_schema) {
      return `${foreignKey.referenced_schema}.${foreignKey.referenced_table}`;
    }
    return foreignKey.referenced_table;
  }, []);

  // Build a unique cache key for a foreign key peek
  const buildPeekKey = useCallback(
    (foreignKey: ForeignKey, value: Value): string => {
      const nsKey = namespace ? `${namespace.database}:${namespace.schema ?? ''}` : 'unknown';
      const valueKey = serializePeekValue(value);
      return `${nsKey}:${foreignKey.referenced_table}:${foreignKey.referenced_column}:${valueKey}`;
    },
    [namespace]
  );

  // Load a foreign key peek if not already cached
  const ensurePeekLoaded = useCallback(
    async (foreignKey: ForeignKey, value: Value) => {
      if (!sessionId || !namespace) return;
      const key = buildPeekKey(foreignKey, value);
      const cached = peekCache.get(key);
      if (cached?.status === 'loading' || cached?.status === 'ready') return;
      if (peekRequests.current.has(key)) return;
      peekRequests.current.add(key);
      updatePeekCache(key, { status: 'loading' });

      try {
        const response = await peekForeignKey(
          sessionId,
          namespace,
          foreignKey,
          value,
          PEEK_QUERY_LIMIT
        );
        if (response.success && response.result) {
          updatePeekCache(key, { status: 'ready', result: response.result });
        } else {
          updatePeekCache(key, {
            status: 'error',
            error: response.error || t('grid.peekFailed', { defaultValue: 'Preview unavailable' }),
          });
        }
      } catch (error) {
        updatePeekCache(key, {
          status: 'error',
          error:
            error instanceof Error
              ? error.message
              : t('grid.peekFailed', { defaultValue: 'Preview unavailable' }),
        });
      } finally {
        peekRequests.current.delete(key);
      }
    },
    [buildPeekKey, namespace, sessionId, t, updatePeekCache, peekCache]
  );

  return {
    peekCache,
    foreignKeyMap,
    buildPeekKey,
    ensurePeekLoaded,
    resolveReferencedNamespace,
    getRelationLabel,
  };
}
