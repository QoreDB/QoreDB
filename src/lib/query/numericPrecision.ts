// SPDX-License-Identifier: Apache-2.0

const INTEGER_TYPES = ['int', 'serial'];
const EXACT_TYPES = ['decimal', 'numeric', 'money'];
// A double is what these columns store, so a double cannot lose anything.
const APPROXIMATE_TYPES = ['float', 'double', 'real'];

function includesAny(dataType: string, needles: string[]): boolean {
  return needles.some(needle => dataType.includes(needle));
}

/** Whether the column holds a value a JavaScript number may not represent. */
export function isExactNumericType(dataType?: string): boolean {
  const normalized = dataType?.toLowerCase() ?? '';
  if (includesAny(normalized, APPROXIMATE_TYPES)) return false;
  return includesAny(normalized, INTEGER_TYPES) || includesAny(normalized, EXACT_TYPES);
}

/** Whether the column holds whole numbers, which the wire can carry exactly. */
export function isIntegerType(dataType?: string): boolean {
  const normalized = dataType?.toLowerCase() ?? '';
  if (includesAny(normalized, APPROXIMATE_TYPES) || includesAny(normalized, EXACT_TYPES)) {
    return false;
  }
  return includesAny(normalized, INTEGER_TYPES);
}

/** Plain decimal digits, without the notations that mean the same number. */
function canonicalDigits(text: string): string | null {
  const trimmed = text.trim();
  if (!/\d/.test(trimmed)) return null;
  const match = /^([+-]?)0*(\d*)(?:\.(\d*?)0*)?$/.exec(trimmed);
  if (!match) return null;
  const digits = match[2] === '' ? '0' : match[2];
  const fraction = match[3] ?? '';
  const sign = match[1] === '-' && !(digits === '0' && fraction === '') ? '-' : '';
  return `${sign}${digits}${fraction ? `.${fraction}` : ''}`;
}

/**
 * Whether `text` survives a trip through a double unchanged.
 *
 * Compares the digit strings, not the values: re-parsing the rendered number
 * would compare a double with itself and never see the loss. Anything not in
 * plain decimal form — exponents above all — counts as not surviving, since
 * there is no way to tell what the column would have stored.
 */
export function survivesDouble(text: string): boolean {
  const before = canonicalDigits(text);
  if (before === null) return false;
  const parsed = Number(text);
  if (!Number.isFinite(parsed)) return false;
  return before === canonicalDigits(String(parsed));
}
