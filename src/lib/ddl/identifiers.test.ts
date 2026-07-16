// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { Driver } from '../connection/drivers';
import { quoteIdentifier } from './identifiers';

describe('quoteIdentifier', () => {
  it('quotes with double quotes on postgres', () => {
    expect(quoteIdentifier('users', Driver.Postgres)).toBe('"users"');
  });

  it('quotes with backticks on mysql', () => {
    expect(quoteIdentifier('users', Driver.Mysql)).toBe('`users`');
  });

  it('quotes with brackets on sql server', () => {
    // The bracket quote chars are regex metacharacters; building a RegExp from
    // them used to throw, breaking every SQL Server identifier.
    expect(quoteIdentifier('users', Driver.SqlServer)).toBe('[users]');
  });

  it('escapes an embedded quote by doubling it', () => {
    expect(quoteIdentifier('we"ird', Driver.Postgres)).toBe('"we""ird"');
    expect(quoteIdentifier('we`ird', Driver.Mysql)).toBe('`we``ird`');
  });

  it('escapes only the closing bracket on sql server', () => {
    // `]` is what ends the identifier; `[` inside it is an ordinary character,
    // so doubling it would corrupt the name.
    expect(quoteIdentifier('a]b', Driver.SqlServer)).toBe('[a]]b]');
    expect(quoteIdentifier('a[b', Driver.SqlServer)).toBe('[a[b]');
  });
});
