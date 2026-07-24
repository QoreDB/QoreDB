// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import type {
  ColumnFilter,
  ColumnInfo,
  Namespace,
  QueryResult,
  Row,
  SortDirection,
} from '@/lib/tauri';
import { queryTable } from '@/lib/tauri';

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
  totalRows: number;
  totalRowsExact: boolean;
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
  const [totalRows, setTotalRows] = useState(0);
  const [totalRowsExact, setTotalRowsExact] = useState(false);
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
  const totalRowsExactRef = useRef(false);
  // Set by refresh() to make the next reload bypass the query cache.
  const bypassCacheRef = useRef(false);

  const fetchNextChunk = useCallback(async () => {
    if (fetchingRef.current || isComplete || !enabled) return;
    fetchingRef.current = true;
    setIsFetchingMore(true);
    setError(null);

    const generation = generationRef.current;
    const page = currentPageRef.current;
    const isFirstChunk = page === 1;

    try {
      const startTime = isFirstChunk ? performance.now() : 0;

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

        if (isFirstChunk) {
          const endTime = performance.now();
          setExecutionTimeMs(paginated.result.execution_time_ms);
          setTotalTimeMs(endTime - startTime);
          setCached(result.cached ?? false);
          setCachedAgeMs(result.cached_age_ms);
        }

        setColumns(prev => {
          const newCols = paginated.result.columns;
          if (newCols.length > 0) return prev.length === 0 ? newCols : prev;
          return prev;
        });
        setAllRows(prev => prev.concat(paginated.result.rows));
        setIsComplete(!paginated.has_more || paginated.result.rows.length === 0);
        if (paginated.total_rows_exact) {
          totalRowsExactRef.current = true;
          setTotalRows(paginated.total_rows);
          setTotalRowsExact(true);
        } else if (!totalRowsExactRef.current) {
          setTotalRows(prev => Math.max(prev, paginated.total_rows));
        }
        currentPageRef.current = page + 1;
      } else if (result.success && !result.result) {
        setColumns([]);
        setAllRows([]);
        setTotalRows(0);
        setIsComplete(true);
      } else if (result.error) {
        setError(result.error);
      }
    } catch (err) {
      if (generationRef.current !== generation) return;
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
  ]);

  const calculateExactTotal = useCallback(async () => {
    if (!enabled || countingTotalRef.current || totalRowsExactRef.current) return;
    countingTotalRef.current = true;
    setIsCountingTotal(true);
    const generation = generationRef.current;

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
        },
        true
      );

      if (generationRef.current !== generation) return;

      if (result.success && result.result?.total_rows_exact) {
        totalRowsExactRef.current = true;
        setTotalRows(result.result.total_rows);
        setTotalRowsExact(true);
      } else {
        toast.error(t('grid.infiniteScroll.countTotalError'), {
          description: result.error,
        });
      }
    } catch (err) {
      if (generationRef.current !== generation) return;
      toast.error(t('grid.infiniteScroll.countTotalError'), {
        description: err instanceof Error ? err.message : undefined,
      });
    } finally {
      if (generationRef.current === generation) {
        countingTotalRef.current = false;
        setIsCountingTotal(false);
      }
    }
  }, [enabled, filters, namespace, searchTerm, sessionId, t, tableName]);

  const reset = useCallback(() => {
    generationRef.current += 1;
    currentPageRef.current = 1;
    fetchingRef.current = false;
    countingTotalRef.current = false;
    totalRowsExactRef.current = false;
    setAllRows([]);
    // Keep columns: table structure doesn't change between searches/sorts
    setTotalRows(0);
    setTotalRowsExact(false);
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
    totalRowsExact,
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
    reload,
    refresh,
  };
}
