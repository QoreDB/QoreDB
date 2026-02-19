// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
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
  loadedRows: number;
  isLoading: boolean;
  isFetchingMore: boolean;
  isComplete: boolean;
  error: string | null;
  fetchNextChunk: () => void;
  reload: () => void;
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
  const [allRows, setAllRows] = useState<Row[]>([]);
  const [columns, setColumns] = useState<ColumnInfo[]>([]);
  const [totalRows, setTotalRows] = useState(0);
  const [isLoading, setIsLoading] = useState(true);
  const [isFetchingMore, setIsFetchingMore] = useState(false);
  const [isComplete, setIsComplete] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Timing: captured from the first chunk only
  const [executionTimeMs, setExecutionTimeMs] = useState(0);
  const [totalTimeMs, setTotalTimeMs] = useState<number | undefined>(undefined);

  const currentPageRef = useRef(1);
  const generationRef = useRef(0);
  const fetchingRef = useRef(false);

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

      const result = await queryTable(sessionId, namespace, tableName, {
        page,
        page_size: chunkSize,
        sort_column: sortColumn,
        sort_direction: sortDirection,
        search: searchTerm,
        filters,
      });

      if (generationRef.current !== generation) return;

      if (result.success && result.result) {
        const paginated = result.result;

        // Capture timing from first chunk
        if (isFirstChunk) {
          const endTime = performance.now();
          setExecutionTimeMs(paginated.result.execution_time_ms);
          setTotalTimeMs(endTime - startTime);
        }

        setColumns(prev => (prev.length === 0 ? paginated.result.columns : prev));
        setAllRows(prev => {
          const next = [...prev, ...paginated.result.rows];
          if (next.length >= paginated.total_rows) {
            setIsComplete(true);
          }
          return next;
        });
        setTotalRows(paginated.total_rows);
        currentPageRef.current = page + 1;

        if (paginated.result.rows.length === 0) {
          setIsComplete(true);
        }
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

  const reset = useCallback(() => {
    generationRef.current += 1;
    currentPageRef.current = 1;
    fetchingRef.current = false;
    setAllRows([]);
    setColumns([]);
    setTotalRows(0);
    setIsLoading(true);
    setIsFetchingMore(false);
    setIsComplete(false);
    setError(null);
    setExecutionTimeMs(0);
    setTotalTimeMs(undefined);
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
    loadedRows: allRows.length,
    isLoading,
    isFetchingMore,
    isComplete,
    error,
    fetchNextChunk,
    reload,
  };
}
