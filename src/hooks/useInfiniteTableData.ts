// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import {
  openPaginationScope,
  recordPaginationError,
  recordPaginationExactCount,
  recordPaginationPage,
} from '@/lib/diagnostics/paginationMetrics';
import type {
  ColumnFilter,
  ColumnInfo,
  Namespace,
  QueryResult,
  Row,
  SortDirection,
  TotalRowsSource,
} from '@/lib/tauri';
import { cancelQuery, queryTable } from '@/lib/tauri';

interface UseInfiniteTableDataOptions {
  sessionId: string;
  namespace: Namespace;
  tableName: string;
  chunkSize?: number;
  sortColumn?: string;
  sortDirection?: SortDirection;
  searchTerm?: string;
  filters?: ColumnFilter[];
  enabled?: boolean;
}

interface UseInfiniteTableDataReturn {
  data: QueryResult | null;
  /** Null while the total is unknown; never a lower bound. */
  totalRows: number | null;
  /** How `totalRows` was obtained. Null whenever `totalRows` is. */
  totalRowsSource: TotalRowsSource | null;
  /** Unix ms of the statistics behind an estimate, when the engine exposes it. */
  totalRowsAsOf: number | null;
  loadedRows: number;
  isLoading: boolean;
  isFetchingMore: boolean;
  isCountingTotal: boolean;
  isComplete: boolean;
  error: string | null;
  /** True when the currently displayed data was served from the query cache. */
  cached: boolean;
  /** Age of the cached entry in milliseconds, when served from cache. */
  cachedAgeMs: number | undefined;
  fetchNextChunk: () => void;
  calculateExactTotal: () => void;
  /** Interrupts a running exact count. No-op when none is running. */
  cancelExactTotal: () => void;
  reload: () => void;
  /** Like reload(), but forces fresh data even when a valid cache entry exists. */
  refresh: () => void;
}

export function useInfiniteTableData({
  sessionId,
  namespace,
  tableName,
  chunkSize = 100,
  sortColumn,
  sortDirection,
  searchTerm,
  filters,
  enabled = true,
}: UseInfiniteTableDataOptions): UseInfiniteTableDataReturn {
  const { t } = useTranslation();
  const [allRows, setAllRows] = useState<Row[]>([]);
  const [columns, setColumns] = useState<ColumnInfo[]>([]);
  const [totalRows, setTotalRows] = useState<number | null>(null);
  const [totalRowsSource, setTotalRowsSource] = useState<TotalRowsSource | null>(null);
  const [totalRowsAsOf, setTotalRowsAsOf] = useState<number | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isFetchingMore, setIsFetchingMore] = useState(false);
  const [isCountingTotal, setIsCountingTotal] = useState(false);
  const [isComplete, setIsComplete] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Timing: captured from the first chunk only
  const [executionTimeMs, setExecutionTimeMs] = useState(0);
  const [totalTimeMs, setTotalTimeMs] = useState<number | undefined>(undefined);

  // Cache status: captured from the first chunk only
  const [cached, setCached] = useState(false);
  const [cachedAgeMs, setCachedAgeMs] = useState<number | undefined>(undefined);

  const currentPageRef = useRef(1);
  const generationRef = useRef(0);
  const fetchingRef = useRef(false);
  const countingTotalRef = useRef(false);
  const knownTotalRef = useRef<number | null>(null);
  const countQueryIdRef = useRef<string | null>(null);
  const countCancelledRef = useRef(false);
  // Created on the first page rather than on mount, so a scope never exists
  // without a measurement in it.
  const scopeRef = useRef<string | null>(null);
  const scope = useCallback(() => (scopeRef.current ??= openPaginationScope()), []);
  // Set by refresh() to make the next reload bypass the query cache.
  const bypassCacheRef = useRef(false);

  // Fired after the first page, never before it: an order of magnitude is
  // worth having, but not at the cost of delaying the rows.
  const fetchEstimate = useCallback(
    async (generation: number) => {
      if (knownTotalRef.current !== null) return;
      try {
        const result = await queryTable(sessionId, namespace, tableName, {
          page: 1,
          page_size: 1,
          search: searchTerm,
          filters,
          count_mode: 'estimated',
        });

        if (generationRef.current !== generation) return;
        // An exact total may have landed while the estimate was in flight.
        if (knownTotalRef.current !== null) return;

        const total = result.result?.total_rows;
        const source = result.result?.total_rows_source;
        if (!result.success || typeof total !== 'number' || !source) return;

        if (source === 'exact') knownTotalRef.current = total;
        setTotalRows(total);
        setTotalRowsSource(source);
        setTotalRowsAsOf(result.result?.total_rows_as_of ?? null);
      } catch {
        // A missing estimate is not a failure the user needs to hear about.
      }
    },
    [filters, namespace, searchTerm, sessionId, tableName]
  );

  const fetchNextChunk = useCallback(async () => {
    if (fetchingRef.current || isComplete || !enabled) return;
    fetchingRef.current = true;
    setIsFetchingMore(true);
    setError(null);

    const generation = generationRef.current;
    const page = currentPageRef.current;
    const isFirstChunk = page === 1;

    try {
      const startTime = performance.now();

      const result = await queryTable(
        sessionId,
        namespace,
        tableName,
        {
          page,
          page_size: chunkSize,
          sort_column: sortColumn,
          sort_direction: sortDirection,
          search: searchTerm,
          filters,
          count_mode: 'none',
        },
        bypassCacheRef.current
      );

      if (generationRef.current !== generation) return;

      if (result.success && result.result) {
        const paginated = result.result;
        const elapsedMs = performance.now() - startTime;
        recordPaginationPage(scope(), elapsedMs, paginated.result.rows.length, Boolean(searchTerm));

        if (isFirstChunk) {
          setExecutionTimeMs(paginated.result.execution_time_ms);
          setTotalTimeMs(elapsedMs);
          setCached(result.cached ?? false);
          setCachedAgeMs(result.cached_age_ms);
          void fetchEstimate(generation);
        }

        setColumns(prev => {
          const newCols = paginated.result.columns;
          if (newCols.length > 0) return prev.length === 0 ? newCols : prev;
          return prev;
        });
        setAllRows(prev => prev.concat(paginated.result.rows));
        setIsComplete(!paginated.has_more || paginated.result.rows.length === 0);
        if (paginated.total_rows !== null) {
          knownTotalRef.current = paginated.total_rows;
          setTotalRows(paginated.total_rows);
          setTotalRowsSource(paginated.total_rows_source);
          setTotalRowsAsOf(paginated.total_rows_as_of);
        }
        currentPageRef.current = page + 1;
      } else if (result.success && !result.result) {
        setColumns([]);
        setAllRows([]);
        setTotalRows(null);
        setTotalRowsSource(null);
        setIsComplete(true);
      } else if (result.error) {
        recordPaginationError(scope());
        setError(result.error);
      }
    } catch (err) {
      if (generationRef.current !== generation) return;
      recordPaginationError(scope());
      setError(err instanceof Error ? err.message : 'Failed to load data');
    } finally {
      if (generationRef.current === generation) {
        fetchingRef.current = false;
        setIsFetchingMore(false);
        setIsLoading(false);
      }
    }
  }, [
    isComplete,
    enabled,
    sessionId,
    namespace,
    tableName,
    chunkSize,
    sortColumn,
    sortDirection,
    searchTerm,
    filters,
    scope,
    fetchEstimate,
  ]);

  const calculateExactTotal = useCallback(async () => {
    if (!enabled || countingTotalRef.current || knownTotalRef.current !== null) return;
    countingTotalRef.current = true;
    setIsCountingTotal(true);
    const generation = generationRef.current;
    const queryId = crypto.randomUUID();
    countQueryIdRef.current = queryId;
    countCancelledRef.current = false;

    try {
      const result = await queryTable(
        sessionId,
        namespace,
        tableName,
        {
          page: 1,
          page_size: 1,
          search: searchTerm,
          filters,
          count_mode: 'exact',
          query_id: queryId,
        },
        true
      );

      if (generationRef.current !== generation) return;

      recordPaginationExactCount(scope(), countCancelledRef.current);
      const total = result.result?.total_rows;
      if (result.success && typeof total === 'number') {
        knownTotalRef.current = total;
        setTotalRows(total);
        setTotalRowsSource('exact');
        setTotalRowsAsOf(null);
      } else if (!countCancelledRef.current) {
        toast.error(t('grid.infiniteScroll.countTotalError'), {
          description: result.error,
        });
      }
    } catch (err) {
      if (generationRef.current !== generation) return;
      if (!countCancelledRef.current) {
        toast.error(t('grid.infiniteScroll.countTotalError'), {
          description: err instanceof Error ? err.message : undefined,
        });
      }
    } finally {
      if (generationRef.current === generation) {
        countingTotalRef.current = false;
        countQueryIdRef.current = null;
        setIsCountingTotal(false);
      }
    }
  }, [enabled, filters, namespace, scope, searchTerm, sessionId, t, tableName]);

  const cancelExactTotal = useCallback(() => {
    const queryId = countQueryIdRef.current;
    if (!queryId) return;
    // Marked before the round-trip so the count's own rejection is not
    // reported to the user as a failure.
    countCancelledRef.current = true;
    cancelQuery(sessionId, queryId).catch(() => {});
  }, [sessionId]);

  const reset = useCallback(() => {
    generationRef.current += 1;
    currentPageRef.current = 1;
    fetchingRef.current = false;
    countingTotalRef.current = false;
    knownTotalRef.current = null;
    setAllRows([]);
    // Keep columns: table structure doesn't change between searches/sorts
    setTotalRows(null);
    setTotalRowsSource(null);
    setTotalRowsAsOf(null);
    setIsLoading(true);
    setIsFetchingMore(false);
    setIsCountingTotal(false);
    setIsComplete(false);
    setError(null);
    setExecutionTimeMs(0);
    setTotalTimeMs(undefined);
    setCached(false);
    setCachedAgeMs(undefined);
  }, []);

  // Reset when sort/search/filters change
  const sortColumnRef = useRef(sortColumn);
  const sortDirectionRef = useRef(sortDirection);
  const searchTermRef = useRef(searchTerm);
  const filtersRef = useRef(filters);

  useEffect(() => {
    const sortChanged =
      sortColumnRef.current !== sortColumn || sortDirectionRef.current !== sortDirection;
    const searchChanged = searchTermRef.current !== searchTerm;
    const filtersChanged = filtersRef.current !== filters;

    sortColumnRef.current = sortColumn;
    sortDirectionRef.current = sortDirection;
    searchTermRef.current = searchTerm;
    filtersRef.current = filters;

    if (sortChanged || searchChanged || filtersChanged) {
      bypassCacheRef.current = false;
      reset();
    }
  }, [sortColumn, sortDirection, searchTerm, filters, reset]);

  // Auto-fetch first chunk on mount or after reset
  useEffect(() => {
    if (!enabled) return;
    if (allRows.length === 0 && !fetchingRef.current && !isComplete) {
      fetchNextChunk();
    }
  }, [enabled, allRows.length, isComplete, fetchNextChunk]);

  const reload = useCallback(() => {
    bypassCacheRef.current = false;
    reset();
  }, [reset]);

  const refresh = useCallback(() => {
    bypassCacheRef.current = true;
    reset();
  }, [reset]);

  const data = useMemo<QueryResult | null>(() => {
    if (columns.length === 0 && allRows.length === 0) return null;
    return {
      columns,
      rows: allRows,
      affected_rows: undefined,
      execution_time_ms: executionTimeMs,
      total_time_ms: totalTimeMs,
    };
  }, [columns, allRows, executionTimeMs, totalTimeMs]);

  return {
    data,
    totalRows,
    totalRowsSource,
    totalRowsAsOf,
    loadedRows: allRows.length,
    isLoading,
    isFetchingMore,
    isCountingTotal,
    isComplete,
    error,
    cached,
    cachedAgeMs,
    fetchNextChunk,
    calculateExactTotal,
    cancelExactTotal,
    reload,
    refresh,
  };
}
