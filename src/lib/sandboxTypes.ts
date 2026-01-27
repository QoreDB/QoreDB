/**
 * Sandbox Types
 *
 * Type definitions for the Sandbox (Bac à sable) feature.
 * Allows users to make local modifications without immediate database impact.
 */

import { Namespace, Value, TableSchema, RowData } from './tauri';

/** Type of sandbox change operation */
export type SandboxChangeType = 'insert' | 'update' | 'delete';

/** Display mode for deleted rows in the grid */
export type SandboxDeleteDisplay = 'strikethrough' | 'hidden';

/**
 * Represents a single change in the sandbox.
 * Can be an insert, update, or delete operation.
 */
export interface SandboxChange {
  /** Unique identifier for this change */
  id: string;
  /** Type of change operation */
  type: SandboxChangeType;
  /** Timestamp when the change was made */
  timestamp: number;
  /** Session ID this change belongs to */
  sessionId: string;
  /** Database namespace (database + optional schema) */
  namespace: Namespace;
  /** Name of the table being modified */
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

/**
 * Represents a sandbox session for a connection.
 * Each connection can have one active sandbox session.
 */
export interface SandboxSession {
  /** Session ID this sandbox is associated with */
  sessionId: string;
  /** Whether sandbox mode is currently active */
  isActive: boolean;
  /** Timestamp when sandbox was activated */
  activatedAt: number;
  /** List of changes made in this sandbox session */
  changes: SandboxChange[];
}

/**
 * Sandbox state stored in localStorage.
 */
export interface SandboxState {
  /** Map of session ID to sandbox session */
  sessions: Record<string, SandboxSession>;
}

/**
 * User preferences for sandbox behavior.
 */
export interface SandboxPreferences {
  /** How to display deleted rows */
  deleteDisplay: SandboxDeleteDisplay;
  /** Show confirmation before discarding changes */
  confirmOnDiscard: boolean;
  /** Auto-collapse changes panel */
  autoCollapsePanel: boolean;
  /** Page size for changes panel pagination */
  panelPageSize: number;
}

/**
 * Grouped changes by table for display in the changes panel.
 */
export interface SandboxChangeGroup {
  /** Namespace of the table */
  namespace: Namespace;
  /** Table name */
  tableName: string;
  /** Display name (schema.table or just table) */
  displayName: string;
  /** Changes for this table */
  changes: SandboxChange[];
  /** Count by type */
  counts: {
    insert: number;
    update: number;
    delete: number;
  };
}

/**
 * DTO sent to backend for SQL generation.
 */
export interface SandboxChangeDto {
  change_type: SandboxChangeType;
  namespace: Namespace;
  table_name: string;
  primary_key?: RowData;
  old_values?: Record<string, Value>;
  new_values?: Record<string, Value>;
}

/**
 * Response from backend SQL generation.
 */
export interface MigrationScript {
  /** Generated SQL script */
  sql: string;
  /** Number of statements in the script */
  statement_count: number;
  /** Warnings about potential issues */
  warnings: string[];
}

/**
 * Response from applying sandbox changes.
 */
export interface ApplySandboxResult {
  /** Whether all changes were applied successfully */
  success: boolean;
  /** Number of changes applied */
  applied_count: number;
  /** Error message if failed */
  error?: string;
  /** Details about each failed change */
  failed_changes?: Array<{
    index: number;
    error: string;
  }>;
}

/**
 * Metadata for visual row highlighting in the grid.
 */
export interface SandboxRowMetadata {
  /** Row has been modified */
  isModified: boolean;
  /** Row is newly inserted */
  isInserted: boolean;
  /** Row is marked for deletion */
  isDeleted: boolean;
  /** Which columns have been modified */
  modifiedColumns: Set<string>;
  /** The sandbox change that affects this row */
  change?: SandboxChange;
}
