// SPDX-License-Identifier: BUSL-1.1

/**
 * Data Contracts — typed model for declarative data-quality assertions.
 *
 * Field naming is snake_case to match the canonical YAML/JSON serialization
 * format and the Rust mirror in `src-tauri/src/contracts/`.
 */

export type Severity = 'error' | 'warning' | 'info';

export interface ContractTarget {
  connection: string;
  schema?: string;
  table: string;
}

export type RuleType =
  | 'not_null_pct'
  | 'not_empty'
  | 'regex_match'
  | 'length_range'
  | 'numeric_range'
  | 'date_range'
  | 'allowed_values'
  | 'unique'
  | 'distinct_count'
  | 'foreign_key_integrity'
  | 'row_count'
  | 'custom_sql';

interface BaseRule {
  id: string;
  type: RuleType;
  description?: string;
  severity?: Severity;
  enabled?: boolean;
}

export interface NotNullPctRule extends BaseRule {
  type: 'not_null_pct';
  column: string;
  threshold_min_pct: number;
}

export interface NotEmptyRule extends BaseRule {
  type: 'not_empty';
  column: string;
}

export interface RegexMatchRule extends BaseRule {
  type: 'regex_match';
  column: string;
  pattern: string;
}

export interface LengthRangeRule extends BaseRule {
  type: 'length_range';
  column: string;
  min?: number;
  max?: number;
}

export interface NumericRangeRule extends BaseRule {
  type: 'numeric_range';
  column: string;
  min?: number;
  max?: number;
  inclusive_min?: boolean;
  inclusive_max?: boolean;
}

export interface DateRangeRule extends BaseRule {
  type: 'date_range';
  column: string;
  min?: string;
  max?: string;
  max_age?: string;
}

export type AllowedValue = string | number | boolean | null;

export interface AllowedValuesRule extends BaseRule {
  type: 'allowed_values';
  column: string;
  values: AllowedValue[];
}

export interface UniqueRule extends BaseRule {
  type: 'unique';
  columns: string[];
}

export interface DistinctCountRule extends BaseRule {
  type: 'distinct_count';
  column: string;
  min?: number;
  max?: number;
}

export interface ForeignKeyReference {
  table: string;
  column: string;
  schema?: string;
}

export interface ForeignKeyIntegrityRule extends BaseRule {
  type: 'foreign_key_integrity';
  column: string;
  references: ForeignKeyReference;
}

export interface RowCountRule extends BaseRule {
  type: 'row_count';
  min?: number;
  max?: number;
}

export interface CustomSqlRule extends BaseRule {
  type: 'custom_sql';
  sql: string;
}

export type Rule =
  | NotNullPctRule
  | NotEmptyRule
  | RegexMatchRule
  | LengthRangeRule
  | NumericRangeRule
  | DateRangeRule
  | AllowedValuesRule
  | UniqueRule
  | DistinctCountRule
  | ForeignKeyIntegrityRule
  | RowCountRule
  | CustomSqlRule;

export interface Contract {
  name: string;
  version: number;
  description?: string;
  target: ContractTarget;
  rules: Rule[];
}

export type RuleStatus = 'pass' | 'fail' | 'skipped' | 'error';

export interface RuleResult {
  id: string;
  rule_type: RuleType;
  status: RuleStatus;
  violations_count?: number;
  metric?: number;
  samples?: Record<string, unknown>[];
  duration_ms: number;
  error?: string;
}

export interface ContractRun {
  contract_id: string;
  contract_name: string;
  connection_id: string;
  started_at: string;
  finished_at: string;
  duration_ms: number;
  pass_count: number;
  fail_count: number;
  error_count: number;
  results: RuleResult[];
}

export interface ContractMeta {
  id: string;
  name: string;
  path: string;
  rules_count: number;
  last_run?: ContractRun;
}
