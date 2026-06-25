// SPDX-License-Identifier: Apache-2.0

/**
 * Type definitions for the Sandbox (Bac à sable) feature.
 * Allows users to make local modifications without immediate database impact.
 */

import type { Namespace, RowData, TableSchema, Value } from '../tauri';

export type SandboxChangeType = 'insert' | 'update' | 'delete';

export type SandboxDeleteDisplay = 'strikethrough' | 'hidden';

export interface SandboxChange {
  id: string;
  type: SandboxChangeType;
  timestamp: number;
  sessionId: string;
  /** Database namespace (database + optional schema) */
  namespace: Namespace;
  tableName: string;
  /** Primary key values to identify the row (for update/delete) */
  primaryKey?: RowData;
  /** Original values before modification (for update) */
  oldValues?: Record<string, Value>;
  /** New values to apply (for insert/update) */
  newValues?: Record<string, Value>;
  /** Table schema at time of change (for validation) */
  schema?: TableSchema;
}

/** A sandbox session for a connection; each connection can have one active. */
export interface SandboxSession {
  sessionId: string;
  isActive: boolean;
  activatedAt: number;
  changes: SandboxChange[];
}

/** Sandbox state stored in localStorage. */
export interface SandboxState {
  sessions: Record<string, SandboxSession>;
}

export interface SandboxPreferences {
  deleteDisplay: SandboxDeleteDisplay;
  confirmOnDiscard: boolean;
  autoCollapsePanel: boolean;
  panelPageSize: number;
}

/** Grouped changes by table for display in the changes panel. */
export interface SandboxChangeGroup {
  namespace: Namespace;
  tableName: string;
  /** Display name (schema.table or just table) */
  displayName: string;
  changes: SandboxChange[];
  counts: {
    insert: number;
    update: number;
    delete: number;
  };
}

/** DTO sent to backend for SQL generation. */
export interface SandboxChangeDto {
  change_type: SandboxChangeType;
  namespace: Namespace;
  table_name: string;
  primary_key?: RowData;
  old_values?: Record<string, Value>;
  new_values?: Record<string, Value>;
}

/** Response from backend SQL generation. */
export interface MigrationScript {
  sql: string;
  statement_count: number;
  warnings: string[];
}

/** Response from applying sandbox changes. */
export interface ApplySandboxResult {
  success: boolean;
  applied_count: number;
  error?: string;
  failed_changes?: Array<{
    index: number;
    error: string;
  }>;
}

/** Metadata for visual row highlighting in the grid. */
export interface SandboxRowMetadata {
  isModified: boolean;
  isInserted: boolean;
  isDeleted: boolean;
  modifiedColumns: Set<string>;
  change?: SandboxChange;
}
