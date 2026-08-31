// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { NON_TX_DDL_DRIVERS, SCHEMA_DIFF_DRIVERS, SCHEMA_MIGRATION_DRIVERS } from './drivers';

describe('wire-compatible migration support', () => {
  it('allows manual SQL migrations for every A1 relational driver', () => {
    for (const driver of [
      'tidb',
      'starrocks',
      'doris',
      'singlestore',
      'yugabytedb',
      'azuresql',
      'synapse',
    ]) {
      expect(SCHEMA_MIGRATION_DRIVERS.has(driver)).toBe(true);
    }
  });

  it('keeps divergent DDL out of schema-diff generation', () => {
    expect(SCHEMA_DIFF_DRIVERS.has('starrocks')).toBe(false);
    expect(SCHEMA_DIFF_DRIVERS.has('doris')).toBe(false);
    expect(SCHEMA_DIFF_DRIVERS.has('synapse')).toBe(false);
    expect(SCHEMA_DIFF_DRIVERS.has('tidb')).toBe(true);
    expect(SCHEMA_DIFF_DRIVERS.has('yugabytedb')).toBe(true);
    expect(SCHEMA_DIFF_DRIVERS.has('azuresql')).toBe(true);
  });

  it('marks MySQL-protocol DDL as non-transactional', () => {
    for (const driver of ['tidb', 'starrocks', 'doris', 'singlestore']) {
      expect(NON_TX_DDL_DRIVERS.has(driver)).toBe(true);
    }
  });
});
