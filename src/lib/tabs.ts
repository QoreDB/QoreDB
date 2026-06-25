// SPDX-License-Identifier: Apache-2.0

import type { Namespace, QueryResult, RelationFilter, SearchFilter } from './tauri';

export type TabType =
  | 'query'
  | 'table'
  | 'database'
  | 'diff'
  | 'federation'
  | 'snapshots'
  | 'notebook'
  | 'time-travel'
  | 'plugin-output';

export interface DiffSource {
  type: 'query' | 'table' | 'snapshot';
  label: string;
  namespace?: Namespace;
  connectionId?: string;
  tableName?: string;
  query?: string;
  snapshotId?: string;
  result?: QueryResult;
}

export interface OpenTab {
  id: string;
  type: TabType;
  title: string;
  pinned?: boolean;
  initialQuery?: string;
  /** Connection this tab belongs to. Used by Tab Groups by Connection. */
  connectionId?: string;
  // Table-specific
  namespace?: Namespace;
  tableName?: string;
  relationFilter?: RelationFilter;
  searchFilter?: SearchFilter;
  // Diff-specific
  diffLeftSource?: DiffSource;
  diffRightSource?: DiffSource;
  // AI-specific
  showAiPanel?: boolean;
  aiTableContext?: string;
  // Notebook-specific
  notebookPath?: string;
  notebookDirty?: boolean;
  // Time-Travel-specific
  timeTravelNamespace?: Namespace;
  timeTravelTableName?: string;
}

export function generateTabId(): string {
  return `tab-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

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

export function createDatabaseTab(namespace: Namespace): OpenTab {
  const title = namespace.schema ? `${namespace.database}.${namespace.schema}` : namespace.database;
  return {
    id: generateTabId(),
    type: 'database',
    title,
    namespace,
  };
}

export function createQueryTab(initialQuery?: string, namespace?: Namespace): OpenTab {
  return {
    id: generateTabId(),
    type: 'query',
    title: 'Query',
    initialQuery,
    namespace,
  };
}

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

export function createFederationTab(initialQuery?: string): OpenTab {
  return {
    id: generateTabId(),
    type: 'federation',
    title: 'Federation',
    initialQuery,
  };
}

export function createSnapshotsTab(): OpenTab {
  return {
    id: generateTabId(),
    type: 'snapshots',
    title: 'Snapshots',
  };
}

export function createNotebookTab(title?: string, path?: string, initialQuery?: string): OpenTab {
  return {
    id: generateTabId(),
    type: 'notebook',
    title: title || 'Untitled Notebook',
    notebookPath: path,
    initialQuery,
  };
}

/** Create the singleton plugin-output tab. The `useTabs` dedup logic (matches
 *  on type + namespace + tableName + connectionId) folds repeat opens onto
 *  the same tab, so callers can call this freely. */
export function createPluginOutputTab(title: string): OpenTab {
  return {
    id: generateTabId(),
    type: 'plugin-output',
    title,
  };
}

export function createTimeTravelTab(namespace: Namespace, tableName: string): OpenTab {
  return {
    id: generateTabId(),
    type: 'time-travel',
    title: `History: ${tableName}`,
    namespace,
    timeTravelNamespace: namespace,
    timeTravelTableName: tableName,
  };
}
