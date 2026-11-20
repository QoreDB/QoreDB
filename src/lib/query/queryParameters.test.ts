// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { Driver } from '@/lib/connection/drivers';
import {
  areAllQueryParametersFilled,
  extractQueryParameters,
  formatQueryParameter,
  substituteQueryParameters,
} from './queryParameters';

describe('extractQueryParameters', () => {
  it('extracts positional, named, braced, colon, and at parameters in order', () => {
    const sql =
      'SELECT * FROM orders WHERE tenant_id = $1 AND user_id = :user_id AND status = @status AND region = {{region}} AND owner = $owner AND backup_id = $1';
    expect(extractQueryParameters(sql)).toEqual(['$1', ':user_id', '@status', 'region', 'owner']);
  });

  it('ignores strings, quoted identifiers, comments, PG casts, dollar quotes, and @@sysvars', () => {
    const sql = `
      -- comment with :ignored_comment and $1
      /* block comment with @ignored_block and {{ignored}} */
      SELECT
        id::text,
        created_at::timestamptz,
        @@global.sql_mode,
        @@IDENTITY,
        'literal :not_a_param $2 @nope {{nope}}' AS txt,
        "quoted_:col" AS q,
        \`backtick_@col\` AS b,
        $$dollar :quoted $3$$ AS dq,
        $fn$SELECT :inside_fn$fn$ AS dq2,
        $users.id AS inter_cell
      FROM users
      WHERE email = :email AND id = $1
    `;
    expect(extractQueryParameters(sql)).toEqual([':email', '$1']);
  });

  it('ignores time literals and Postgres array slices', () => {
    const sql = "SELECT '12:30:00', arr[1:2] FROM logs WHERE ts > :since";
    expect(extractQueryParameters(sql)).toEqual([':since']);
  });
});

describe('substituteQueryParameters', () => {
  it('formats text, exact numbers, ISO dates, and NULL safely in a single pass', () => {
    const sql =
      'SELECT * FROM orders WHERE id = $1 AND total = :total AND created_at >= @since AND note = {{note}} AND deleted_at IS :deleted';
    const result = substituteQueryParameters(sql, {
      $1: { value: '9007199254740993', type: 'number' },
      ':total': { value: '42.50', type: 'number' },
      '@since': { value: '2026-11-19', type: 'date' },
      note: { value: "O'Brien :total $1\nline2", type: 'text' },
      ':deleted': { value: '', type: 'null' },
    });
    expect(result).toBe(
      "SELECT * FROM orders WHERE id = 9007199254740993 AND total = 42.50 AND created_at >= '2026-11-19' AND note = 'O''Brien :total $1line2' AND deleted_at IS NULL"
    );
  });

  it('leaves unfilled or invalid parameters untouched without injecting', () => {
    const sql = 'SELECT * FROM t WHERE a = :num AND b = :dt AND c = :missing';
    const result = substituteQueryParameters(sql, {
      ':num': { value: '1 OR 1=1', type: 'number' },
      ':dt': { value: "2026-01-01'; DROP TABLE t;--", type: 'date' },
    });
    expect(result).toBe(sql);
  });

  it('infers numbers, booleans, NULL, and quoted strings in auto mode', () => {
    const sql =
      'SELECT * FROM orders WHERE status = :status AND total_cents >= {{min_cents}} AND active = $1 AND deleted_at IS :del';
    const values = {
      ':status': { value: 'paid', type: 'auto' as const },
      min_cents: { value: '1000', type: 'auto' as const },
      $1: { value: 'true', type: 'auto' as const },
      ':del': { value: 'null', type: 'auto' as const },
    };
    expect(areAllQueryParametersFilled(sql, values)).toBe(true);
    expect(substituteQueryParameters(sql, values)).toBe(
      "SELECT * FROM orders WHERE status = 'paid' AND total_cents >= 1000 AND active = TRUE AND deleted_at IS NULL"
    );
    expect(
      areAllQueryParametersFilled(sql, {
        ...values,
        min_cents: { value: '  ', type: 'auto' },
      })
    ).toBe(false);
  });

  it('does not alter parameters inside strings, comments, or casts', () => {
    const sql = "SELECT ':id'::text, $1 FROM t -- :id";
    expect(
      substituteQueryParameters(sql, {
        ':id': { value: 'foo', type: 'text' },
        $1: { value: '7', type: 'number' },
      })
    ).toBe("SELECT ':id'::text, 7 FROM t -- :id");
  });
});

describe('dialect-aware parameters', () => {
  it('treats @name as a native variable on SQL Server and MySQL-family drivers', () => {
    const tsql =
      'DECLARE @limit INT = 10; SELECT TOP (@limit) * FROM orders WHERE region = :region';
    expect(extractQueryParameters(tsql, Driver.SqlServer)).toEqual([':region']);
    expect(
      extractQueryParameters('SET @rank := 0; SELECT @rank := @rank + 1', Driver.Mysql)
    ).toEqual([]);
    expect(extractQueryParameters('SELECT * FROM t WHERE a = @a', Driver.Postgres)).toEqual(['@a']);
  });

  it('leaves T-SQL variables untouched when substituting', () => {
    const tsql = 'DECLARE @x INT = 1; SELECT @x, :name';
    expect(
      substituteQueryParameters(tsql, { ':name': { value: 'Bob', type: 'text' } }, Driver.SqlServer)
    ).toBe("DECLARE @x INT = 1; SELECT @x, 'Bob'");
    expect(areAllQueryParametersFilled('DECLARE @x INT = 1; SELECT @x', {}, Driver.AzureSql)).toBe(
      true
    );
  });

  it('skips backslash-escaped quotes inside MySQL strings', () => {
    const sql = "SELECT 'it\\'s :not_a_param' AS s FROM t WHERE id = :id";
    expect(extractQueryParameters(sql, Driver.Mysql)).toEqual([':id']);
  });

  it('escapes backslashes for MySQL-family drivers', () => {
    const value = { value: "x\\' OR 1=1 -- ", type: 'text' as const };
    expect(formatQueryParameter(value, Driver.Mariadb)).toBe("'x\\\\'' OR 1=1 -- '");
    expect(formatQueryParameter(value, Driver.Postgres)).toBe("'x\\'' OR 1=1 -- '");
  });

  it('formats auto booleans as bits on SQL Server', () => {
    expect(formatQueryParameter({ value: 'true', type: 'auto' }, Driver.SqlServer)).toBe('1');
    expect(formatQueryParameter({ value: 'false', type: 'auto' }, Driver.Postgres)).toBe('FALSE');
  });

  it('keeps leading-zero codes as text in auto mode', () => {
    expect(formatQueryParameter({ value: '007', type: 'auto' })).toBe("'007'");
    expect(formatQueryParameter({ value: '0.5', type: 'auto' })).toBe('0.5');
    expect(formatQueryParameter({ value: '0', type: 'auto' })).toBe('0');
    expect(formatQueryParameter({ value: '007', type: 'number' })).toBe('007');
  });
});
