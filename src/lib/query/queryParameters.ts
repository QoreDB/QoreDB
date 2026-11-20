// SPDX-License-Identifier: Apache-2.0

import {
  isMySqlFamily,
  isSqlServerFamily,
  usesBackslashStringEscapes,
} from '@/lib/connection/driverCapabilities';
import type { Driver } from '@/lib/connection/drivers';
import { quoteSqlString } from './sqlStringLiteral';

export type QueryParameterType = 'auto' | 'text' | 'number' | 'date' | 'null';

export interface QueryParameterValue {
  value: string;
  type: QueryParameterType;
}

interface ParameterMatch {
  key: string;
  start: number;
  end: number;
}

// biome-ignore lint/suspicious/noControlCharactersInRegex: intentional — strips control characters that could prematurely terminate a SQL statement
const SQL_QUOTE_STRIP_RE = /[\u0000\r\n\u2028\u2029]/g;
const NUMBER_LITERAL_RE = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;
/** `007`, `01234`: codes and zip codes, not numbers. */
const LEADING_ZERO_RE = /^[+-]?0\d/;
const DATE_LITERAL_RE = /^\d{4}-\d{2}-\d{2}(?:[T ][\d:.+\-Z]+)?$/;

const DOLLAR_TAG_RE = /^\$([a-zA-Z_][a-zA-Z0-9_]*)?\$/;
const BRACED_PARAM_RE = /^\{\{(\w+)\}\}/;
const POSITIONAL_PARAM_RE = /^\$(\d+)(?![a-zA-Z0-9_.])/;
const DOLLAR_NAME_PARAM_RE = /^\$([a-zA-Z_][a-zA-Z0-9_]*)(\.\w+)?/;
const COLON_PARAM_RE = /^:([a-zA-Z_][a-zA-Z0-9_]*)/;
const AT_PARAM_RE = /^@([a-zA-Z_][a-zA-Z0-9_]*)/;

function isIdentChar(ch: string | undefined): boolean {
  return ch !== undefined && /[a-zA-Z0-9_]/.test(ch);
}

function sqlQuote(value: string, driver?: Driver | string): string {
  return quoteSqlString(value.replace(SQL_QUOTE_STRIP_RE, ''), driver);
}

/** T-SQL and MySQL use `@name` for their own variables, never as a placeholder. */
function hasNativeAtVariables(driver?: Driver | string): boolean {
  return Boolean(driver && (isSqlServerFamily(driver) || isMySqlFamily(driver)));
}

export function formatQueryParameter(
  param: QueryParameterValue,
  driver?: Driver | string
): string | null {
  switch (param.type) {
    case 'null':
      return 'NULL';
    case 'auto': {
      const trimmed = param.value.trim();
      if (trimmed.length === 0) return null;
      if (NUMBER_LITERAL_RE.test(trimmed) && !LEADING_ZERO_RE.test(trimmed)) return trimmed;
      const lower = trimmed.toLowerCase();
      if (lower === 'null') return 'NULL';
      if (lower === 'true' || lower === 'false') {
        if (driver && isSqlServerFamily(driver)) return lower === 'true' ? '1' : '0';
        return lower.toUpperCase();
      }
      return sqlQuote(param.value, driver);
    }
    case 'number': {
      const trimmed = param.value.trim();
      return NUMBER_LITERAL_RE.test(trimmed) ? trimmed : null;
    }
    case 'date': {
      const trimmed = param.value.trim();
      return DATE_LITERAL_RE.test(trimmed) ? sqlQuote(trimmed, driver) : null;
    }
    case 'text':
      return param.value.length > 0 ? sqlQuote(param.value, driver) : null;
    default:
      return null;
  }
}

function scanQueryParameters(sql: string, driver?: Driver | string): ParameterMatch[] {
  const matches: ParameterMatch[] = [];
  const len = sql.length;
  const backslashEscapes = Boolean(driver && usesBackslashStringEscapes(driver));
  const atParameters = !hasNativeAtVariables(driver);
  let i = 0;

  while (i < len) {
    const ch = sql[i];

    // Single-quoted string literal (handles '' escapes)
    if (ch === "'") {
      i++;
      while (i < len) {
        if (backslashEscapes && sql[i] === '\\') {
          i += 2;
          continue;
        }
        if (sql[i] === "'") {
          if (sql[i + 1] === "'") {
            i += 2;
            continue;
          }
          i++;
          break;
        }
        i++;
      }
      continue;
    }

    // Double-quoted identifier (handles "" escapes)
    if (ch === '"') {
      i++;
      while (i < len) {
        if (sql[i] === '"') {
          if (sql[i + 1] === '"') {
            i += 2;
            continue;
          }
          i++;
          break;
        }
        i++;
      }
      continue;
    }

    // Backtick-quoted identifier (handles `` escapes)
    if (ch === '`') {
      i++;
      while (i < len) {
        if (sql[i] === '`') {
          if (sql[i + 1] === '`') {
            i += 2;
            continue;
          }
          i++;
          break;
        }
        i++;
      }
      continue;
    }

    // Line comment: -- ...
    if (ch === '-' && sql[i + 1] === '-') {
      i += 2;
      while (i < len && sql[i] !== '\n') i++;
      continue;
    }

    // Block comment: /* ... */ (supports nesting)
    if (ch === '/' && sql[i + 1] === '*') {
      i += 2;
      let depth = 1;
      while (i < len && depth > 0) {
        if (sql[i] === '/' && sql[i + 1] === '*') {
          depth++;
          i += 2;
        } else if (sql[i] === '*' && sql[i + 1] === '/') {
          depth--;
          i += 2;
        } else {
          i++;
        }
      }
      continue;
    }

    const rest = sql.slice(i);
    const prev = i > 0 ? sql[i - 1] : undefined;

    // {{name}}
    if (ch === '{' && sql[i + 1] === '{') {
      const m = BRACED_PARAM_RE.exec(rest);
      if (m) {
        matches.push({ key: m[1], start: i, end: i + m[0].length });
        i += m[0].length;
        continue;
      }
    }

    // Dollar-quoted strings ($tag$...$tag$ or $$...$$) and $1 / $name
    if (ch === '$') {
      const tagMatch = DOLLAR_TAG_RE.exec(rest);
      if (tagMatch) {
        const tag = tagMatch[0];
        const closeIdx = sql.indexOf(tag, i + tag.length);
        i = closeIdx >= 0 ? closeIdx + tag.length : len;
        continue;
      }

      if (!isIdentChar(prev)) {
        const posMatch = POSITIONAL_PARAM_RE.exec(rest);
        if (posMatch) {
          matches.push({ key: posMatch[0], start: i, end: i + posMatch[0].length });
          i += posMatch[0].length;
          continue;
        }

        const nameMatch = DOLLAR_NAME_PARAM_RE.exec(rest);
        if (nameMatch) {
          if (!nameMatch[2]) {
            matches.push({ key: nameMatch[1], start: i, end: i + nameMatch[0].length });
          }
          i += nameMatch[0].length;
          continue;
        }
      }
    }

    // :name (skip ::type casts)
    if (ch === ':') {
      if (sql[i + 1] === ':') {
        while (i < len && sql[i] === ':') i++;
        continue;
      }
      if (!isIdentChar(prev)) {
        const colonMatch = COLON_PARAM_RE.exec(rest);
        if (colonMatch) {
          matches.push({ key: colonMatch[0], start: i, end: i + colonMatch[0].length });
          i += colonMatch[0].length;
          continue;
        }
      }
    }

    // @name (skip @@sysvar)
    if (ch === '@') {
      if (sql[i + 1] === '@') {
        while (i < len && sql[i] === '@') i++;
        continue;
      }
      if (atParameters && !isIdentChar(prev)) {
        const atMatch = AT_PARAM_RE.exec(rest);
        if (atMatch) {
          matches.push({ key: atMatch[0], start: i, end: i + atMatch[0].length });
          i += atMatch[0].length;
          continue;
        }
      }
    }

    i++;
  }

  return matches;
}

/**
 * Extract SQL parameters (`{{name}}`, `$1`, `$name`, `:name`, `@name`) in order
 * of first appearance, ignoring string literals, quoted identifiers, comments,
 * PostgreSQL casts (`::type`), dollar-quoted blocks (`$$...$$`), MySQL system
 * variables (`@@var`), and notebook inter-cell references (`$label.col`).
 * With a SQL Server or MySQL-family `driver`, `@name` is a native variable.
 */
export function extractQueryParameters(sql: string, driver?: Driver | string): string[] {
  const seen = new Set<string>();
  const ordered: string[] = [];
  for (const match of scanQueryParameters(sql, driver)) {
    if (!seen.has(match.key)) {
      seen.add(match.key);
      ordered.push(match.key);
    }
  }
  return ordered;
}

export function areAllQueryParametersFilled(
  sql: string,
  params: Record<string, QueryParameterValue>,
  driver?: Driver | string
): boolean {
  const keys = extractQueryParameters(sql, driver);
  if (keys.length === 0) return true;
  return keys.every(key => {
    // biome-ignore lint/suspicious/noPrototypeBuiltins: the TypeScript target predates ES2022
    if (!Object.prototype.hasOwnProperty.call(params, key)) return false;
    return formatQueryParameter(params[key], driver) !== null;
  });
}

/**
 * Substitute filled SQL parameters in a single pass so substituted values are
 * never re-scanned as placeholders. Unfilled or invalid parameters remain intact.
 */
export function substituteQueryParameters(
  sql: string,
  params: Record<string, QueryParameterValue>,
  driver?: Driver | string
): string {
  const matches = scanQueryParameters(sql, driver);
  if (matches.length === 0) return sql;

  let out = '';
  let cursor = 0;
  for (const match of matches) {
    out += sql.slice(cursor, match.start);
    // biome-ignore lint/suspicious/noPrototypeBuiltins: the TypeScript target predates ES2022
    const hasParam = Object.prototype.hasOwnProperty.call(params, match.key);
    const formatted = hasParam ? formatQueryParameter(params[match.key], driver) : null;
    out += formatted ?? sql.slice(match.start, match.end);
    cursor = match.end;
  }
  out += sql.slice(cursor);
  return out;
}
