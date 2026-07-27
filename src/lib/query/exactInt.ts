// SPDX-License-Identifier: Apache-2.0

import type { Value } from '@/lib/tauri';

/**
 * Envelope the engine uses for an integer a JSON number cannot hold.
 *
 * Kept as-is in memory rather than converted to a number or a bigint: the value
 * is sent back untouched when a row is deleted or edited, so the `WHERE` clause
 * carries the integer the row actually has. Converting it would reintroduce the
 * rounding the envelope exists to avoid.
 */
export const EXACT_INT_KEY = '$qoreInt';

export interface ExactInt {
  [EXACT_INT_KEY]: string;
}

export function isExactInt(value: unknown): value is ExactInt {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
  const keys = Object.keys(value);
  return (
    keys.length === 1 &&
    keys[0] === EXACT_INT_KEY &&
    typeof (value as Record<string, unknown>)[EXACT_INT_KEY] === 'string'
  );
}

/** The digits, for display, comparison and row identity. */
export function exactIntText(value: ExactInt): string {
  return value[EXACT_INT_KEY];
}

/**
 * `JSON.stringify` replacer that unwraps envelopes to their digits.
 *
 * They leave as JSON strings rather than as bare numbers: a number would be
 * exact in the file but rounded again the moment anything parses it, which is
 * the whole problem. Quoting 64-bit identifiers is what most APIs carrying them
 * already do.
 */
export function exactIntReplacer(_key: string, value: unknown): unknown {
  return isExactInt(value) ? exactIntText(value) : value;
}

/**
 * The envelope when `text` needs one, a plain number otherwise.
 *
 * Used on the way back from an edit: sending a rounded number for a value the
 * user typed exactly would write the wrong integer.
 */
export function toExactValue(text: string): Value {
  const parsed = Number(text);
  if (Number.isSafeInteger(parsed) && String(parsed) === text.trim()) return parsed;
  return { [EXACT_INT_KEY]: text.trim() };
}
