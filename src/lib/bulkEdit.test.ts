// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { buildBulkEditChanges, coerceValueForColumn } from './bulkEdit';
import type { TableSchema } from './tauri';

describe('bulk edit numeric precision', () => {
  it.each([
    'bigint',
    'BIGSERIAL',
    'int8',
  ])('preserves large signed integers in %s columns', dataType => {
    for (const raw of [
      '9007199254740992',
      '9007199254740993',
      '-9007199254740993',
      '9223372036854775807',
    ]) {
      expect(coerceValueForColumn(raw, dataType)).toEqual({ $qoreInt: raw });
    }
  });

  it.each([
    'numeric(40,20)',
    'DECIMAL(40,20)',
    'money',
  ])('keeps exact %s decimals as text when a number would round them', dataType => {
    const raw = '1234567890123456789.12345678901234567890';
    expect(coerceValueForColumn(raw, dataType)).toBe(raw);
  });

  it.each([
    ['42', 'integer', 42],
    ['1.50', 'numeric', 1.5],
    ['0.25', 'decimal', 0.25],
    ['1.25e2', 'double precision', 125],
    ['1', 'bool', true],
    ['f', 'boolean', false],
    ['NULL', 'text', 'NULL'],
    ['', 'bigint', ''],
    ['   ', 'numeric', '   '],
    ['invalid', 'integer', 'invalid'],
    ['Infinity', 'real', 'Infinity'],
    ['9007199254740993e0', 'bigint', '9007199254740993e0'],
    ['0.123456789012345678901e0', 'numeric', '0.123456789012345678901e0'],
    ['0x20000000000001', 'bigint', '0x20000000000001'],
  ])('retains existing coercion for %s (%s)', (raw, dataType, expected) => {
    expect(coerceValueForColumn(raw, dataType)).toBe(expected);
  });

  it.each(['bigint', 'decimal(40,20)'])('preserves %s through DTO JSON serialization', dataType => {
    const schema: TableSchema = {
      columns: [{ name: 'amount', data_type: dataType, nullable: true, is_primary_key: false }],
      primary_key: ['id'],
      foreign_keys: [],
      indexes: [],
    };
    const raw = dataType === 'bigint' ? '9007199254740993' : '0.123456789012345678901';
    const args = {
      plan: { column: 'amount', operation: 'set_value' as const, value: raw },
      rows: [{ id: { $qoreInt: '9223372036854775807' }, amount: null }],
      namespace: { database: 'test' },
      tableName: 'items',
      primaryKey: ['id'],
      tableSchema: schema,
    };
    const changes = JSON.parse(JSON.stringify(buildBulkEditChanges(args)));
    expect(changes).toHaveLength(1);
    expect(changes[0].new_values.amount).toEqual(dataType === 'bigint' ? { $qoreInt: raw } : raw);
    expect(changes[0].primary_key.columns.id).toEqual({ $qoreInt: '9223372036854775807' });
    expect(changes[0].old_values.amount).toBeNull();
    expect(
      buildBulkEditChanges({ ...args, plan: { ...args.plan, operation: 'set_null' } })[0].new_values
        ?.amount
    ).toBeNull();
  });
});
