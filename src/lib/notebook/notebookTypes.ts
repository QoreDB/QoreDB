// SPDX-License-Identifier: Apache-2.0

import type { ColumnInfo, Namespace, Row } from '../tauri';

// --- File format types (serialized to .qnb) ---

export interface QoreNotebook {
  version: 1;
  metadata: NotebookMetadata;
  cells: NotebookCell[];
  variables: Record<string, NotebookVariable>;
}

export interface NotebookMetadata {
  id: string;
  title: string;
  description?: string;
  createdAt: string;
  updatedAt: string;
  author?: string;
  tags?: string[];
  connectionHint?: {
    driver: string;
    database?: string;
    label?: string;
  };
}

export type CellType = 'sql' | 'mongo' | 'markdown' | 'chart';

export interface NotebookCell {
  id: string;
  type: CellType;
  source: string;
  lastResult?: CellResult | null;
  executionState?: CellExecutionState;
  executionCount?: number;
  executedAt?: string;
  executionTimeMs?: number;
  config?: CellConfig;
}

export type CellExecutionState = 'idle' | 'running' | 'success' | 'error' | 'stale';

export interface CellConfig {
  namespace?: Namespace;
  maxRows?: number;
  collapsed?: boolean;
  pinned?: boolean;
  label?: string;
  hideSource?: boolean;
  chartConfig?: ChartConfig;
}

export interface ChartConfig {
  sourceLabel: string;
  type: 'bar' | 'line' | 'pie' | 'scatter';
  xColumn: string;
  yColumns: string[];
  title?: string;
}

export interface CellResult {
  type: 'table' | 'document' | 'message' | 'error';
  columns?: ColumnInfo[];
  rows?: Row[];
  totalRows?: number;
  affectedRows?: number;
  documents?: object[];
  error?: string;
  message?: string;
}

export interface NotebookVariable {
  name: string;
  type: 'text' | 'number' | 'date' | 'select';
  defaultValue?: string;
  description?: string;
  options?: string[];
  currentValue?: string;
}

// --- Factory helpers ---

export function createEmptyNotebook(title?: string): QoreNotebook {
  return {
    version: 1,
    metadata: {
      id: crypto.randomUUID(),
      title: title || 'Untitled Notebook',
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    },
    cells: [createCell('sql')],
    variables: {},
  };
}

export function createCell(type: CellType, source?: string): NotebookCell {
  return {
    id: crypto.randomUUID(),
    type,
    source: source ?? '',
    executionState: 'idle',
    executionCount: 0,
  };
}
