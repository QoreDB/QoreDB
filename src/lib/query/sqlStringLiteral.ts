// SPDX-License-Identifier: Apache-2.0

import { usesBackslashStringEscapes } from '@/lib/connection/driverCapabilities';
import type { Driver } from '@/lib/connection/drivers';

/**
 * Quotes `value` as a single-quoted SQL string literal for `driver`. NUL bytes
 * are dropped; dialects that read `\` as an escape get it doubled, otherwise
 * `x\'; DROP …` would close the literal early.
 */
export function quoteSqlString(value: string, driver?: Driver | string): string {
  let escaped = value.replace(/\0/g, '');
  if (driver && usesBackslashStringEscapes(driver)) {
    escaped = escaped.replace(/\\/g, '\\\\');
  }
  return `'${escaped.replace(/'/g, "''")}'`;
}
