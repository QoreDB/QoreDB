/**
 * Interceptor Tauri API
 *
 * TypeScript bindings for the Rust Universal Query Interceptor.
 * All data is stored and processed in the backend for maximum security.
 */

import { invoke } from '@tauri-apps/api/core';

// ============================================
// TYPES
// ============================================

export type Environment = 'development' | 'staging' | 'production';

export type QueryOperationType =
  | 'select'
  | 'insert'
  | 'update'
  | 'delete'
  | 'create'
  | 'alter'
  | 'drop'
  | 'truncate'
  | 'grant'
  | 'revoke'
  | 'execute'
  | 'other';

export type SafetyAction = 'block' | 'warn' | 'require_confirmation';

export interface SafetyRule {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  environments: Environment[];
  operations: QueryOperationType[];
  action: SafetyAction;
  pattern?: string;
  builtin: boolean;
}

export const BUILTIN_SAFETY_RULE_I18N: Record<string, { nameKey: string; descriptionKey: string }> =
  {
    'builtin-no-drop-production': {
      nameKey: 'interceptor.safety.builtinRuleNames.builtin-no-drop-production',
      descriptionKey: 'interceptor.safety.builtinRuleDescriptions.builtin-no-drop-production',
    },
    'builtin-no-truncate-production': {
      nameKey: 'interceptor.safety.builtinRuleNames.builtin-no-truncate-production',
      descriptionKey: 'interceptor.safety.builtinRuleDescriptions.builtin-no-truncate-production',
    },
    'builtin-confirm-delete-production': {
      nameKey: 'interceptor.safety.builtinRuleNames.builtin-confirm-delete-production',
      descriptionKey:
        'interceptor.safety.builtinRuleDescriptions.builtin-confirm-delete-production',
    },
    'builtin-confirm-update-no-where': {
      nameKey: 'interceptor.safety.builtinRuleNames.builtin-confirm-update-no-where',
      descriptionKey: 'interceptor.safety.builtinRuleDescriptions.builtin-confirm-update-no-where',
    },
    'builtin-confirm-delete-no-where': {
      nameKey: 'interceptor.safety.builtinRuleNames.builtin-confirm-delete-no-where',
      descriptionKey: 'interceptor.safety.builtinRuleDescriptions.builtin-confirm-delete-no-where',
    },
    'builtin-warn-alter-production': {
      nameKey: 'interceptor.safety.builtinRuleNames.builtin-warn-alter-production',
      descriptionKey: 'interceptor.safety.builtinRuleDescriptions.builtin-warn-alter-production',
    },
  };

export interface AuditLogEntry {
  id: string;
  timestamp: string;
  session_id: string;
  query: string;
  query_preview: string;
  environment: Environment;
  operation_type: QueryOperationType;
  database?: string;
  success: boolean;
  error?: string;
  execution_time_ms: number;
  row_count?: number;
  blocked: boolean;
  safety_rule?: string;
  driver_id: string;
}

export interface AuditStats {
  total: number;
  successful: number;
  failed: number;
  blocked: number;
  last_hour: number;
  last_day: number;
  by_environment: Record<string, number>;
  by_operation: Record<string, number>;
}

export interface ProfilingMetrics {
  total_queries: number;
  successful_queries: number;
  failed_queries: number;
  blocked_queries: number;
  total_execution_time_ms: number;
  avg_execution_time_ms: number;
  min_execution_time_ms: number;
  max_execution_time_ms: number;
  p50_execution_time_ms: number;
  p95_execution_time_ms: number;
  p99_execution_time_ms: number;
  slow_query_count: number;
  by_operation_type: Record<string, number>;
  by_environment: Record<string, number>;
  period_start: string;
}

export interface SlowQueryEntry {
  id: string;
  timestamp: string;
  query: string;
  execution_time_ms: number;
  environment: Environment;
  database?: string;
  row_count?: number;
  driver_id: string;
}

export interface InterceptorConfig {
  audit_enabled: boolean;
  profiling_enabled: boolean;
  safety_enabled: boolean;
  slow_query_threshold_ms: number;
  max_audit_entries: number;
  max_slow_queries: number;
  safety_rules: SafetyRule[];
  builtin_rule_overrides: BuiltinRuleOverride[];
}

export interface BuiltinRuleOverride {
  id: string;
  enabled: boolean;
}

export interface AuditFilter {
  limit?: number;
  offset?: number;
  environment?: Environment;
  operation?: QueryOperationType;
  success?: boolean;
  search?: string;
}

// ============================================
// RESPONSE TYPES
// ============================================

interface InterceptorConfigResponse {
  success: boolean;
  config?: InterceptorConfig;
  error?: string;
}

interface AuditEntriesResponse {
  success: boolean;
  entries: AuditLogEntry[];
  error?: string;
}

interface AuditStatsResponse {
  success: boolean;
  stats?: AuditStats;
  error?: string;
}

interface ProfilingMetricsResponse {
  success: boolean;
  metrics?: ProfilingMetrics;
  error?: string;
}

interface SlowQueriesResponse {
  success: boolean;
  queries: SlowQueryEntry[];
  error?: string;
}

interface SafetyRulesResponse {
  success: boolean;
  rules: SafetyRule[];
  error?: string;
}

interface GenericResponse {
  success: boolean;
  error?: string;
}

interface ExportResponse {
  success: boolean;
  data?: string;
  error?: string;
}

// ============================================
// CONFIGURATION API
// ============================================

/**
 * Get the current interceptor configuration
 */
export async function getInterceptorConfig(): Promise<InterceptorConfig> {
  const result = await invoke<InterceptorConfigResponse>('get_interceptor_config');
  if (!result.success || !result.config) {
    throw new Error(result.error || 'Failed to get interceptor config');
  }
  return result.config;
}

/**
 * Update the interceptor configuration
 */
export async function updateInterceptorConfig(
  config: InterceptorConfig
): Promise<InterceptorConfig> {
  const result = await invoke<InterceptorConfigResponse>('update_interceptor_config', { config });
  if (!result.success || !result.config) {
    throw new Error(result.error || 'Failed to update interceptor config');
  }
  return result.config;
}

// ============================================
// AUDIT LOG API
// ============================================

/**
 * Get audit log entries with optional filtering
 */
export async function getAuditEntries(filter: AuditFilter = {}): Promise<AuditLogEntry[]> {
  const result = await invoke<AuditEntriesResponse>('get_audit_entries', { filter });
  if (!result.success) {
    throw new Error(result.error || 'Failed to get audit entries');
  }
  return result.entries;
}

/**
 * Get audit log statistics
 */
export async function getAuditStats(): Promise<AuditStats> {
  const result = await invoke<AuditStatsResponse>('get_audit_stats');
  if (!result.success || !result.stats) {
    throw new Error(result.error || 'Failed to get audit stats');
  }
  return result.stats;
}

/**
 * Clear the audit log
 */
export async function clearAuditLog(): Promise<void> {
  const result = await invoke<GenericResponse>('clear_audit_log');
  if (!result.success) {
    throw new Error(result.error || 'Failed to clear audit log');
  }
}

/**
 * Export audit log as JSON string
 */
export async function exportAuditLog(): Promise<string> {
  const result = await invoke<ExportResponse>('export_audit_log');
  if (!result.success || !result.data) {
    throw new Error(result.error || 'Failed to export audit log');
  }
  return result.data;
}

// ============================================
// PROFILING API
// ============================================

/**
 * Get current profiling metrics
 */
export async function getProfilingMetrics(): Promise<ProfilingMetrics> {
  const result = await invoke<ProfilingMetricsResponse>('get_profiling_metrics');
  if (!result.success || !result.metrics) {
    throw new Error(result.error || 'Failed to get profiling metrics');
  }
  return result.metrics;
}

/**
 * Get slow query entries
 */
export async function getSlowQueries(limit = 50, offset = 0): Promise<SlowQueryEntry[]> {
  const result = await invoke<SlowQueriesResponse>('get_slow_queries', { limit, offset });
  if (!result.success) {
    throw new Error(result.error || 'Failed to get slow queries');
  }
  return result.queries;
}

/**
 * Clear slow query entries
 */
export async function clearSlowQueries(): Promise<void> {
  const result = await invoke<GenericResponse>('clear_slow_queries');
  if (!result.success) {
    throw new Error(result.error || 'Failed to clear slow queries');
  }
}

/**
 * Reset all profiling metrics
 */
export async function resetProfilingMetrics(): Promise<void> {
  const result = await invoke<GenericResponse>('reset_profiling');
  if (!result.success) {
    throw new Error(result.error || 'Failed to reset profiling');
  }
}

/**
 * Export profiling data as JSON string
 */
export async function exportProfilingData(): Promise<string> {
  const result = await invoke<ExportResponse>('export_profiling');
  if (!result.success || !result.data) {
    throw new Error(result.error || 'Failed to export profiling data');
  }
  return result.data;
}

// ============================================
// SAFETY RULES API
// ============================================

/**
 * Get all safety rules (built-in + custom)
 */
export async function getSafetyRules(): Promise<SafetyRule[]> {
  const result = await invoke<SafetyRulesResponse>('get_safety_rules');
  if (!result.success) {
    throw new Error(result.error || 'Failed to get safety rules');
  }
  return result.rules;
}

/**
 * Add a custom safety rule
 */
export async function addSafetyRule(rule: SafetyRule): Promise<SafetyRule[]> {
  const result = await invoke<SafetyRulesResponse>('add_safety_rule', { rule });
  if (!result.success) {
    throw new Error(result.error || 'Failed to add safety rule');
  }
  return result.rules;
}

/**
 * Update an existing safety rule
 */
export async function updateSafetyRule(rule: SafetyRule): Promise<SafetyRule[]> {
  const result = await invoke<SafetyRulesResponse>('update_safety_rule', { rule });
  if (!result.success) {
    throw new Error(result.error || 'Failed to update safety rule');
  }
  return result.rules;
}

/**
 * Remove a custom safety rule
 */
export async function removeSafetyRule(ruleId: string): Promise<SafetyRule[]> {
  const result = await invoke<SafetyRulesResponse>('remove_safety_rule', { ruleId });
  if (!result.success) {
    throw new Error(result.error || 'Failed to remove safety rule');
  }
  return result.rules;
}

// ============================================
// UTILITY FUNCTIONS
// ============================================

/**
 * Format execution time for display
 */
export function formatExecutionTime(ms: number): string {
  if (ms < 1) {
    return `${(ms * 1000).toFixed(0)}µs`;
  } else if (ms < 1000) {
    return `${ms.toFixed(1)}ms`;
  } else {
    return `${(ms / 1000).toFixed(2)}s`;
  }
}

/**
 * Get performance class based on execution time
 */
export function getPerformanceClass(ms: number): 'fast' | 'normal' | 'slow' | 'critical' {
  if (ms < 100) return 'fast';
  if (ms < 500) return 'normal';
  if (ms < 2000) return 'slow';
  return 'critical';
}

/**
 * Get color for performance class
 */
export function getPerformanceColor(perfClass: 'fast' | 'normal' | 'slow' | 'critical'): string {
  switch (perfClass) {
    case 'fast':
      return '#22c55e'; // green-500
    case 'normal':
      return '#3b82f6'; // blue-500
    case 'slow':
      return '#f59e0b'; // amber-500
    case 'critical':
      return '#ef4444'; // red-500
  }
}
