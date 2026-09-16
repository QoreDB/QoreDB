// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { Driver } from '@/lib/connection/drivers';
import { buildConnectionConfig, getMissingRequirements, isConnectionFormValid } from './mappers';
import { initialConnectionFormData } from './types';

describe('CQL connection requirements', () => {
  it.each([Driver.Cassandra, Driver.ScyllaDb])('allows %s without authentication', driver => {
    const form = { ...initialConnectionFormData, driver, host: '127.0.0.1', port: 9042 };
    expect(getMissingRequirements(form)).toEqual([]);
    expect(isConnectionFormValid(form)).toBe(true);
    expect(buildConnectionConfig(form)).toMatchObject({ username: '', password: '' });
  });

  it.each([Driver.Cassandra, Driver.ScyllaDb])('preserves %s credentials when provided', driver => {
    const form = {
      ...initialConnectionFormData,
      driver,
      port: 9042,
      username: 'cassandra',
      password: 'test-password',
    };
    expect(isConnectionFormValid(form)).toBe(true);
    expect(buildConnectionConfig(form)).toMatchObject({
      username: 'cassandra',
      password: 'test-password',
    });
  });

  it('still requires a valid host and port for Cassandra', () => {
    expect(
      getMissingRequirements({
        ...initialConnectionFormData,
        driver: Driver.Cassandra,
        host: '',
        port: 0,
      })
    ).toEqual(['connection.host', 'connection.port']);
  });

  it.each([
    Driver.Postgres,
    Driver.Mysql,
    Driver.Snowflake,
  ])('still requires a username for %s', driver => {
    expect(getMissingRequirements({ ...initialConnectionFormData, driver })).toContain(
      'connection.username'
    );
  });
});
