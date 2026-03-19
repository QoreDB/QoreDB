// SPDX-License-Identifier: Apache-2.0

import type { NotebookVariable } from './notebookTypes';

/**
 * Replace `$name` and `{{name}}` patterns with variable values.
 * Unresolved patterns are left as-is.
 */
export function substituteVariables(
  source: string,
  variables: Record<string, NotebookVariable>
): string {
  let result = source;

  // Replace {{name}} patterns
  result = result.replace(/\{\{(\w+)\}\}/g, (match, name: string) => {
    const v = variables[name];
    if (!v) return match;
    return v.currentValue ?? v.defaultValue ?? match;
  });

  // Replace $name patterns (word boundary, not preceded by another $)
  result = result.replace(/(?<!\$)\$(\w+)/g, (match, name: string) => {
    const v = variables[name];
    if (!v) return match;
    return v.currentValue ?? v.defaultValue ?? match;
  });

  return result;
}

/**
 * Extract variable names referenced in a cell source.
 */
export function extractVariableReferences(source: string): string[] {
  const names = new Set<string>();

  // Match {{name}}
  for (const m of source.matchAll(/\{\{(\w+)\}\}/g)) {
    names.add(m[1]);
  }

  // Match $name
  for (const m of source.matchAll(/(?<!\$)\$(\w+)/g)) {
    names.add(m[1]);
  }

  return [...names];
}
