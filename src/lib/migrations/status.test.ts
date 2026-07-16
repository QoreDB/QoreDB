// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { nextMigrationDirection } from './status';

describe('nextMigrationDirection', () => {
  it('rolls back an applied migration', () => {
    expect(nextMigrationDirection({ status: 'applied', failed_direction: null })).toBe('down');
  });

  it('resumes the direction of a failed rollback', () => {
    expect(nextMigrationDirection({ status: 'failed', failed_direction: 'down' })).toBe('down');
  });

  it('retries up after a failed apply', () => {
    expect(nextMigrationDirection({ status: 'failed', failed_direction: 'up' })).toBe('up');
  });

  it('applies pending and rolled-back migrations', () => {
    expect(nextMigrationDirection({ status: 'pending', failed_direction: null })).toBe('up');
    expect(nextMigrationDirection({ status: 'rolled_back', failed_direction: null })).toBe('up');
  });
});
