// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import type { NotebookVariable } from './notebookTypes';
import { extractVariableReferences, substituteVariables } from './notebookVariables';

function variable(
  name: string,
  value: string,
  type: NotebookVariable['type'] = 'text'
): NotebookVariable {
  return { name, type, currentValue: value };
}

describe('substituteVariables', () => {
  it('never rescans placeholders supplied by a variable value', () => {
    const variables = {
      first: variable('first', '$second {{second}}'),
      second: variable('second', "O'Brien"),
    };
    expect(substituteVariables('SELECT {{first}}, $first, {{second}}, $second', variables)).toBe(
      "SELECT '$second {{second}}', '$second {{second}}', 'O''Brien', 'O''Brien'"
    );
  });

  it.each([
    '9007199254740993',
    '1234567890.12345678901234567890',
    '-0.00000000000000000001',
    '1.234567890123456789e+30',
  ])('preserves the exact numeric literal %s', value => {
    expect(
      substituteVariables('SELECT {{amount}}', { amount: variable('amount', value, 'number') })
    ).toBe(`SELECT ${value}`);
  });

  it.each([
    '',
    '  ',
    '0x10',
    'Infinity',
    'NaN',
    '1; DROP TABLE users',
    '1_000',
    '1e',
  ])('leaves invalid numeric input unresolved: %s', value => {
    expect(
      substituteVariables('SELECT $amount', { amount: variable('amount', value, 'number') })
    ).toBe('SELECT $amount');
  });

  it('leaves inter-cell references, unknown variables and dollar escapes untouched', () => {
    expect(
      substituteVariables('SELECT $users.id, $$users, {{missing}}, $missing', {
        users: variable('users', 'value'),
      })
    ).toBe('SELECT $users.id, $$users, {{missing}}, $missing');
  });

  it('uses defaults and preserves empty text values', () => {
    expect(
      substituteVariables('SELECT {{default}}, $empty', {
        default: { name: 'default', type: 'text', defaultValue: 'fallback' },
        empty: { name: 'empty', type: 'text', currentValue: '', defaultValue: 'fallback' },
      })
    ).toBe("SELECT 'fallback', ''");
  });

  it('quotes text and dates while stripping SQL control characters', () => {
    expect(
      substituteVariables('SELECT $text, {{date}}', {
        text: variable('text', "O'\nBrien\0"),
        date: variable('date', '2026-10-06', 'date'),
      })
    ).toBe("SELECT 'O''Brien', '2026-10-06'");
  });
});

describe('extractVariableReferences', () => {
  it('excludes inter-cell references, dollar escapes and inherited object names', () => {
    expect(
      extractVariableReferences('SELECT $users.id, $$escaped, {{amount}}, $amount, $text')
    ).toEqual(['amount', 'text']);
    expect(substituteVariables('SELECT {{toString}}, $constructor, {{__proto__}}', {})).toBe(
      'SELECT {{toString}}, $constructor, {{__proto__}}'
    );
  });
});
