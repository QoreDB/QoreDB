// SPDX-License-Identifier: Apache-2.0

import type { NotebookVariable } from './notebookTypes';

/**
 * Replace `$name` and `{{name}}` patterns with variable values.
 *
 * Values are formatted per variable type so the substitution cannot break
 * out of the literal it is replacing (cf. audit B9-C1):
 *
 *  - `number`: rejected unless the value parses as a finite number; substituted
 *    as the bare numeric literal.
 *  - `date`: rejected unless it parses as an ISO-8601 date / timestamp;
 *    substituted as a SQL string literal.
 *  - `text` / `select`: substituted as a SQL string literal with `'`
 *    doubled and any embedded NUL / line terminators stripped.
 *
 * Unresolved patterns (or invalid values) are left as-is. The caller is
 * expected to wrap `{{name}}`/`$name` placeholders in SQL contexts where a
 * literal is allowed — the substitution does NOT inject identifiers.
 */
export function substituteVariables(
  source: string,
  variables: Record<string, NotebookVariable>
): string {
  const replace = (match: string, name: string): string => {
    const v = variables[name];
    if (!v) return match;
    const raw = v.currentValue ?? v.defaultValue;
    if (raw === undefined || raw === null) return match;
    const formatted = formatVariable(v, String(raw));
    return formatted ?? match;
  };

  let result = source;
  result = result.replace(/\{\{(\w+)\}\}/g, replace);
  result = result.replace(/(?<!\$)\$(\w+)/g, replace);
  return result;
}

function formatVariable(variable: NotebookVariable, raw: string): string | null {
  switch (variable.type) {
    case 'number': {
      const n = Number(raw);
      if (!Number.isFinite(n)) return null;
      return String(n);
    }
    case 'date': {
      // Accept YYYY-MM-DD and ISO-8601 timestamps. We re-emit as a quoted
      // string literal so the SQL site `WHERE ts > {{when}}` becomes
      // `WHERE ts > '2026-05-16'` rather than a bare token the parser may
      // reinterpret.
      const trimmed = raw.trim();
      if (!/^\d{4}-\d{2}-\d{2}([T ][\d:.+\-Z]+)?$/.test(trimmed)) return null;
      return sqlQuote(trimmed);
    }
    case 'text':
    case 'select':
      return sqlQuote(raw);
    default:
      return null;
  }
}

// Characters that can prematurely end a SQL statement on at least one
// supported dialect (NUL byte; LF / CR for some shells; U+2028 / U+2029
// line/paragraph separators for JavaScript-aware tooling). Regular spaces
// are preserved so legitimate "Foo Bar" text values still work.
// biome-ignore lint/suspicious/noControlCharactersInRegex: intentional — strips control characters that could prematurely terminate a SQL statement
const SQL_QUOTE_STRIP_RE = /[\u0000\r\n\u2028\u2029]/g;

function sqlQuote(value: string): string {
  const sanitised = value.replace(SQL_QUOTE_STRIP_RE, '');
  return `'${sanitised.replace(/'/g, "''")}'`;
}

export function extractVariableReferences(source: string): string[] {
  const names = new Set<string>();

  for (const m of source.matchAll(/\{\{(\w+)\}\}/g)) {
    names.add(m[1]);
  }

  for (const m of source.matchAll(/(?<!\$)\$(\w+)/g)) {
    names.add(m[1]);
  }

  return [...names];
}
