// SPDX-License-Identifier: Apache-2.0

/**
 * Lightweight linter for MongoDB shell-style queries.
 *
 * The editor accepts `db.<col>.<method>(<json-ish>)` with zero or more
 * JSON-ish arguments separated by commas. We do a best-effort validation:
 *
 *  - Extract the argument region (between the outermost `(` and `)`).
 *  - Split on top-level commas (ignoring those inside strings/objects/arrays).
 *  - Try to parse each segment as JSON after normalising trailing commas,
 *    JS-style comments, and single-quoted strings (mongosh accepts them).
 *  - Report the first JSON.parse error as a diagnostic range.
 *
 * This is intentionally tolerant — we don't want to flag user intent that
 * the backend parser will accept (like single-quoted strings). We only
 * flag structural mistakes that would make the driver bail.
 */

import { type Diagnostic, linter } from '@codemirror/lint';
import type { EditorView } from '@codemirror/view';

export interface MongoLintOptions {
  getErrorMessage?: (raw: string) => string;
}

function normaliseForJson(src: string): string {
  // Strip `//` line and `/* */` block comments — mongosh tolerates both.
  let out = src.replace(/\/\/[^\n]*/g, '').replace(/\/\*[\s\S]*?\*\//g, '');
  // Replace single-quoted strings with double-quoted (best effort, no
  // quote-escape handling — we only care about syntactic shape).
  out = out.replace(/'([^'\\]*(?:\\.[^'\\]*)*)'/g, (_, inner) => {
    const escaped = String(inner).replace(/"/g, '\\"');
    return `"${escaped}"`;
  });
  // Drop trailing commas before `}` or `]`.
  out = out.replace(/,(\s*[}\]])/g, '$1');
  return out;
}

/** Top-level `(` → matching `)` offsets. Null if unbalanced/absent. */
function findArgRegion(src: string): { start: number; end: number } | null {
  let depth = 0;
  let start = -1;
  let inString: string | null = null;
  for (let i = 0; i < src.length; i++) {
    const ch = src[i];
    if (inString) {
      if (ch === '\\') {
        i++;
        continue;
      }
      if (ch === inString) inString = null;
      continue;
    }
    if (ch === '"' || ch === "'") {
      inString = ch;
      continue;
    }
    if (ch === '(') {
      if (depth === 0) start = i + 1;
      depth++;
    } else if (ch === ')') {
      depth--;
      if (depth === 0 && start >= 0) {
        return { start, end: i };
      }
    }
  }
  return null;
}

/** Split a string on top-level commas (outside strings/objects/arrays). */
function splitTopLevelCommas(src: string): Array<{ start: number; end: number }> {
  const parts: Array<{ start: number; end: number }> = [];
  let depth = 0;
  let start = 0;
  let inString: string | null = null;
  for (let i = 0; i < src.length; i++) {
    const ch = src[i];
    if (inString) {
      if (ch === '\\') {
        i++;
        continue;
      }
      if (ch === inString) inString = null;
      continue;
    }
    if (ch === '"' || ch === "'") {
      inString = ch;
      continue;
    }
    if (ch === '{' || ch === '[' || ch === '(') depth++;
    else if (ch === '}' || ch === ']' || ch === ')') depth--;
    else if (ch === ',' && depth === 0) {
      parts.push({ start, end: i });
      start = i + 1;
    }
  }
  parts.push({ start, end: src.length });
  return parts;
}

function isBlank(s: string): boolean {
  return /^\s*$/.test(s);
}

function lintDoc(view: EditorView, opts: MongoLintOptions): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  const doc = view.state.doc.toString();
  if (isBlank(doc)) return diagnostics;

  const region = findArgRegion(doc);
  if (!region) return diagnostics;

  const inside = doc.slice(region.start, region.end);
  const pieces = splitTopLevelCommas(inside);

  for (const piece of pieces) {
    const raw = inside.slice(piece.start, piece.end);
    if (isBlank(raw)) continue;
    const trimmed = raw.trimStart();
    // Only validate object/array literals. Other primitives (strings,
    // numbers, identifiers referring to saved variables) are accepted.
    if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) continue;
    const normalised = normaliseForJson(raw);
    try {
      JSON.parse(normalised);
    } catch (err) {
      const message = opts.getErrorMessage
        ? opts.getErrorMessage(err instanceof Error ? err.message : String(err))
        : err instanceof Error
          ? err.message
          : String(err);
      diagnostics.push({
        from: region.start + piece.start,
        to: region.start + piece.end,
        severity: 'error',
        message,
      });
      // Report only the first error per argument to keep the gutter clean.
    }
  }

  return diagnostics;
}

export function mongoLinter(opts: MongoLintOptions = {}) {
  return linter(view => lintDoc(view, opts));
}
