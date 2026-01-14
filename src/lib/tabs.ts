/**
 * Tab system types for QoreDB
 * Defines the structure of open tabs for multi-table navigation
 */

import { Namespace } from './tauri';

export type TabType = 'query' | 'table' | 'database';

export interface OpenTab {
  id: string;
  type: TabType;
  title: string;
  // Table-specific
  namespace?: Namespace;
  tableName?: string;
}

/** Generate unique tab ID */
export function generateTabId(): string {
  return `tab-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

/** Create a table tab */
export function createTableTab(namespace: Namespace, tableName: string): OpenTab {
  return {
    id: generateTabId(),
    type: 'table',
    title: tableName,
    namespace,
    tableName,
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
export function createQueryTab(): OpenTab {
  return {
    id: generateTabId(),
    type: 'query',
    title: 'Query',
  };
}
