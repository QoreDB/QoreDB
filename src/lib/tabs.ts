/**
 * Tab system types for QoreDB
 * Defines the structure of open tabs for multi-table navigation
 */

import { Namespace, RelationFilter, SearchFilter, QueryResult } from './tauri';

export type TabType = 'query' | 'table' | 'database' | 'diff';

export interface DiffSource {
  type: 'query' | 'table';
  label: string;
  namespace?: Namespace;
  connectionId?: string;
  tableName?: string;
  query?: string;
  result?: QueryResult;
}

export interface OpenTab {
  id: string;
  type: TabType;
  title: string;
  initialQuery?: string;
  // Table-specific
  namespace?: Namespace;
  tableName?: string;
  relationFilter?: RelationFilter;
  searchFilter?: SearchFilter;
  // Diff-specific
  diffLeftSource?: DiffSource;
  diffRightSource?: DiffSource;
}

/** Generate unique tab ID */
export function generateTabId(): string {
  return `tab-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

/** Create a table tab */
export function createTableTab(
  namespace: Namespace,
  tableName: string,
  relationFilter?: RelationFilter,
  searchFilter?: SearchFilter
): OpenTab {
  return {
    id: generateTabId(),
    type: 'table',
    title: tableName,
    namespace,
    tableName,
    relationFilter,
    searchFilter,
  };
}

/** Create a database overview tab */
export function createDatabaseTab(namespace: Namespace): OpenTab {
  const title = namespace.schema 
    ? `${namespace.database}.${namespace.schema}`
    : namespace.database;
  return {
    id: generateTabId(),
    type: 'database',
    title,
    namespace,
  };
}

/** Create a query tab */
export function createQueryTab(initialQuery?: string, namespace?: Namespace): OpenTab {
  return {
    id: generateTabId(),
    type: 'query',
    title: 'Query',
    initialQuery,
    namespace,
  };
}

/** Create a diff tab for comparing two data sources */
export function createDiffTab(
  leftSource?: DiffSource,
  rightSource?: DiffSource,
  title?: string,
  namespace?: Namespace
): OpenTab {
  return {
    id: generateTabId(),
    type: 'diff',
    title: title ?? 'Data Diff',
    namespace: namespace ?? leftSource?.namespace ?? rightSource?.namespace,
    diffLeftSource: leftSource,
    diffRightSource: rightSource,
  };
}
