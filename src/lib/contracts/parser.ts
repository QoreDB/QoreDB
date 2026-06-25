// SPDX-License-Identifier: BUSL-1.1

/**
 * Contract parser — YAML/JSON source → validated `Contract`.
 *
 * Validates structure, required fields, and rule-type-specific invariants
 * (e.g. regex compiles, numeric_range has at least one bound).
 */

import { parse as parseYaml } from 'yaml';
import type {
  AllowedValue,
  Contract,
  ContractTarget,
  ForeignKeyReference,
  Rule,
  RuleType,
  Severity,
} from './types';

export class ContractParseError extends Error {
  readonly path: string;
  readonly cause?: Error;

  constructor(message: string, path: string = '$', cause?: Error) {
    super(path === '$' ? message : `${path}: ${message}`);
    this.name = 'ContractParseError';
    this.path = path;
    this.cause = cause;
  }
}

export type ContractFormat = 'yaml' | 'json' | 'auto';

const SEVERITIES: ReadonlySet<Severity> = new Set(['error', 'warning', 'info']);

const RULE_TYPES: ReadonlySet<RuleType> = new Set([
  'not_null_pct',
  'not_empty',
  'regex_match',
  'length_range',
  'numeric_range',
  'date_range',
  'allowed_values',
  'unique',
  'distinct_count',
  'foreign_key_integrity',
  'row_count',
  'custom_sql',
]);

const IDENT_RE = /^[A-Za-z_][A-Za-z0-9_]*$/;
const DURATION_RE = /^(\d+)(ms|s|m|h|d)$/;

export function parseContract(source: string, format: ContractFormat = 'auto'): Contract {
  if (typeof source !== 'string' || source.trim() === '') {
    throw new ContractParseError('Source is empty.');
  }

  const raw = decodeSource(source, format);
  return validateContract(raw);
}

function decodeSource(source: string, format: ContractFormat): unknown {
  if (format === 'json') return parseJsonStrict(source);
  if (format === 'yaml') return parseYamlStrict(source);

  const trimmed = source.trimStart();
  if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
    try {
      return JSON.parse(source);
    } catch {
      // fall through to YAML — JSON is a YAML subset
    }
  }
  return parseYamlStrict(source);
}

function parseJsonStrict(source: string): unknown {
  try {
    return JSON.parse(source);
  } catch (err) {
    throw new ContractParseError(`Invalid JSON: ${(err as Error).message}`, '$', err as Error);
  }
}

function parseYamlStrict(source: string): unknown {
  try {
    return parseYaml(source, { prettyErrors: true });
  } catch (err) {
    throw new ContractParseError(`Invalid YAML: ${(err as Error).message}`, '$', err as Error);
  }
}

function validateContract(raw: unknown): Contract {
  const obj = expectObject(raw, '$');

  const name = expectString(obj.name, '$.name');
  if (!IDENT_RE.test(name)) {
    throw new ContractParseError(
      'Must match [A-Za-z_][A-Za-z0-9_]* (used as filename + identifier).',
      '$.name'
    );
  }

  const version = expectInt(obj.version, '$.version');
  if (version !== 1) {
    throw new ContractParseError(`Unsupported version ${version} (expected 1).`, '$.version');
  }

  const description = optionalString(obj.description, '$.description');
  const target = validateTarget(obj.target);

  const rulesRaw = obj.rules;
  if (!Array.isArray(rulesRaw) || rulesRaw.length === 0) {
    throw new ContractParseError('Must be a non-empty array.', '$.rules');
  }

  const seenIds = new Set<string>();
  const rules: Rule[] = rulesRaw.map((r, i) => {
    const rule = validateRule(r, `$.rules[${i}]`);
    if (seenIds.has(rule.id)) {
      throw new ContractParseError(`Duplicate rule id "${rule.id}".`, `$.rules[${i}].id`);
    }
    seenIds.add(rule.id);
    return rule;
  });

  return { name, version, description, target, rules };
}

function validateTarget(raw: unknown): ContractTarget {
  const obj = expectObject(raw, '$.target');
  const connection = expectString(obj.connection, '$.target.connection');
  const table = expectString(obj.table, '$.target.table');
  const schema = optionalString(obj.schema, '$.target.schema');
  return { connection, schema, table };
}

function validateRule(raw: unknown, path: string): Rule {
  const obj = expectObject(raw, path);
  const id = expectString(obj.id, `${path}.id`);
  if (!IDENT_RE.test(id)) {
    throw new ContractParseError('Must match [A-Za-z_][A-Za-z0-9_]*.', `${path}.id`);
  }
  const type = expectString(obj.type, `${path}.type`);
  if (!RULE_TYPES.has(type as RuleType)) {
    throw new ContractParseError(`Unknown rule type "${type}".`, `${path}.type`);
  }
  const description = optionalString(obj.description, `${path}.description`);
  const severity = optionalSeverity(obj.severity, `${path}.severity`);
  const enabled = optionalBool(obj.enabled, `${path}.enabled`);
  const base = { id, description, severity, enabled };

  switch (type as RuleType) {
    case 'not_null_pct': {
      const column = expectString(obj.column, `${path}.column`);
      const threshold_min_pct = expectNumber(obj.threshold_min_pct, `${path}.threshold_min_pct`);
      if (threshold_min_pct < 0 || threshold_min_pct > 100) {
        throw new ContractParseError('Must be in [0, 100].', `${path}.threshold_min_pct`);
      }
      return { ...base, type: 'not_null_pct', column, threshold_min_pct };
    }
    case 'not_empty': {
      const column = expectString(obj.column, `${path}.column`);
      return { ...base, type: 'not_empty', column };
    }
    case 'regex_match': {
      const column = expectString(obj.column, `${path}.column`);
      const pattern = expectString(obj.pattern, `${path}.pattern`);
      try {
        new RegExp(pattern);
      } catch (err) {
        throw new ContractParseError(`Invalid regex: ${(err as Error).message}`, `${path}.pattern`);
      }
      return { ...base, type: 'regex_match', column, pattern };
    }
    case 'length_range': {
      const column = expectString(obj.column, `${path}.column`);
      const min = optionalInt(obj.min, `${path}.min`);
      const max = optionalInt(obj.max, `${path}.max`);
      requireAtLeastOne(min, max, `${path}`, 'min', 'max');
      if (min !== undefined && max !== undefined && min > max) {
        throw new ContractParseError('min must be <= max.', path);
      }
      return { ...base, type: 'length_range', column, min, max };
    }
    case 'numeric_range': {
      const column = expectString(obj.column, `${path}.column`);
      const min = optionalNumber(obj.min, `${path}.min`);
      const max = optionalNumber(obj.max, `${path}.max`);
      requireAtLeastOne(min, max, `${path}`, 'min', 'max');
      if (min !== undefined && max !== undefined && min > max) {
        throw new ContractParseError('min must be <= max.', path);
      }
      const inclusive_min = optionalBool(obj.inclusive_min, `${path}.inclusive_min`);
      const inclusive_max = optionalBool(obj.inclusive_max, `${path}.inclusive_max`);
      return { ...base, type: 'numeric_range', column, min, max, inclusive_min, inclusive_max };
    }
    case 'date_range': {
      const column = expectString(obj.column, `${path}.column`);
      const min = optionalString(obj.min, `${path}.min`);
      const max = optionalString(obj.max, `${path}.max`);
      const max_age = optionalString(obj.max_age, `${path}.max_age`);
      if (min === undefined && max === undefined && max_age === undefined) {
        throw new ContractParseError('Provide at least one of min, max, max_age.', path);
      }
      if (max_age !== undefined && !DURATION_RE.test(max_age)) {
        throw new ContractParseError('Must be <number><ms|s|m|h|d>, e.g. "7d".', `${path}.max_age`);
      }
      return { ...base, type: 'date_range', column, min, max, max_age };
    }
    case 'allowed_values': {
      const column = expectString(obj.column, `${path}.column`);
      const values = expectArray(obj.values, `${path}.values`);
      if (values.length === 0) {
        throw new ContractParseError('Must be non-empty.', `${path}.values`);
      }
      const checked: AllowedValue[] = values.map((v, i) => {
        if (v === null) return null;
        const t = typeof v;
        if (t === 'string' || t === 'number' || t === 'boolean') return v as AllowedValue;
        throw new ContractParseError(
          'Only string/number/boolean/null allowed.',
          `${path}.values[${i}]`
        );
      });
      return { ...base, type: 'allowed_values', column, values: checked };
    }
    case 'unique': {
      const columns = expectArray(obj.columns, `${path}.columns`);
      if (columns.length === 0) {
        throw new ContractParseError('Must be non-empty.', `${path}.columns`);
      }
      const checked = columns.map((c, i) => expectString(c, `${path}.columns[${i}]`));
      return { ...base, type: 'unique', columns: checked };
    }
    case 'distinct_count': {
      const column = expectString(obj.column, `${path}.column`);
      const min = optionalInt(obj.min, `${path}.min`);
      const max = optionalInt(obj.max, `${path}.max`);
      requireAtLeastOne(min, max, `${path}`, 'min', 'max');
      if (min !== undefined && max !== undefined && min > max) {
        throw new ContractParseError('min must be <= max.', path);
      }
      return { ...base, type: 'distinct_count', column, min, max };
    }
    case 'foreign_key_integrity': {
      const column = expectString(obj.column, `${path}.column`);
      const references = validateFkRef(obj.references, `${path}.references`);
      return { ...base, type: 'foreign_key_integrity', column, references };
    }
    case 'row_count': {
      const min = optionalInt(obj.min, `${path}.min`);
      const max = optionalInt(obj.max, `${path}.max`);
      requireAtLeastOne(min, max, `${path}`, 'min', 'max');
      if (min !== undefined && max !== undefined && min > max) {
        throw new ContractParseError('min must be <= max.', path);
      }
      return { ...base, type: 'row_count', min, max };
    }
    case 'custom_sql': {
      const sql = expectString(obj.sql, `${path}.sql`);
      if (sql.trim() === '') {
        throw new ContractParseError('Must be non-empty.', `${path}.sql`);
      }
      return { ...base, type: 'custom_sql', sql };
    }
  }
}

function validateFkRef(raw: unknown, path: string): ForeignKeyReference {
  const obj = expectObject(raw, path);
  return {
    table: expectString(obj.table, `${path}.table`),
    column: expectString(obj.column, `${path}.column`),
    schema: optionalString(obj.schema, `${path}.schema`),
  };
}

function expectObject(v: unknown, path: string): Record<string, unknown> {
  if (v === null || typeof v !== 'object' || Array.isArray(v)) {
    throw new ContractParseError('Must be an object.', path);
  }
  return v as Record<string, unknown>;
}

function expectArray(v: unknown, path: string): unknown[] {
  if (!Array.isArray(v)) {
    throw new ContractParseError('Must be an array.', path);
  }
  return v;
}

function expectString(v: unknown, path: string): string {
  if (typeof v !== 'string' || v.length === 0) {
    throw new ContractParseError('Must be a non-empty string.', path);
  }
  return v;
}

function optionalString(v: unknown, path: string): string | undefined {
  if (v === undefined || v === null) return undefined;
  if (typeof v !== 'string') {
    throw new ContractParseError('Must be a string.', path);
  }
  return v;
}

function expectNumber(v: unknown, path: string): number {
  if (typeof v !== 'number' || !Number.isFinite(v)) {
    throw new ContractParseError('Must be a finite number.', path);
  }
  return v;
}

function optionalNumber(v: unknown, path: string): number | undefined {
  if (v === undefined || v === null) return undefined;
  return expectNumber(v, path);
}

function expectInt(v: unknown, path: string): number {
  const n = expectNumber(v, path);
  if (!Number.isInteger(n)) {
    throw new ContractParseError('Must be an integer.', path);
  }
  return n;
}

function optionalInt(v: unknown, path: string): number | undefined {
  if (v === undefined || v === null) return undefined;
  return expectInt(v, path);
}

function optionalBool(v: unknown, path: string): boolean | undefined {
  if (v === undefined || v === null) return undefined;
  if (typeof v !== 'boolean') {
    throw new ContractParseError('Must be a boolean.', path);
  }
  return v;
}

function optionalSeverity(v: unknown, path: string): Severity | undefined {
  if (v === undefined || v === null) return undefined;
  if (typeof v !== 'string' || !SEVERITIES.has(v as Severity)) {
    throw new ContractParseError('Must be "error", "warning", or "info".', path);
  }
  return v as Severity;
}

function requireAtLeastOne(
  a: unknown,
  b: unknown,
  path: string,
  nameA: string,
  nameB: string
): void {
  if (a === undefined && b === undefined) {
    throw new ContractParseError(`Provide at least one of ${nameA}, ${nameB}.`, path);
  }
}
