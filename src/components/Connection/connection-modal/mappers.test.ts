// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { Driver } from '@/lib/connection/drivers';
import {
  buildConnectionConfig,
  buildSaveConnectionInput,
  getMissingRequirements,
  isConnectionFormValid,
} from './mappers';
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

describe('SSH tunnel authentication', () => {
  const sshForm = {
    ...initialConnectionFormData,
    driver: Driver.Postgres,
    host: '127.0.0.1',
    username: 'qoredb',
    useSshTunnel: true,
    sshHost: 'bastion.example.com',
    sshUsername: 'ssh_user',
  };

  it('requires a key path in key mode only', () => {
    expect(getMissingRequirements(sshForm)).toContain('connection.ssh.keyPath');
    expect(getMissingRequirements({ ...sshForm, sshAuthMethod: 'agent' as const })).not.toContain(
      'connection.ssh.keyPath'
    );
  });

  it('delegates to the agent without a key path', () => {
    const form = { ...sshForm, sshAuthMethod: 'agent' as const, sshKeyPath: '/ignored' };
    expect(buildConnectionConfig(form).ssh_tunnel?.auth).toEqual({
      Agent: { identity_agent: undefined },
    });

    const saved = buildSaveConnectionInput(form, 'conn-1');
    expect(saved.ssh_tunnel).toMatchObject({ auth_type: 'agent', key_path: undefined });
  });

  it('forwards a trimmed agent socket override', () => {
    const form = {
      ...sshForm,
      sshAuthMethod: 'agent' as const,
      sshIdentityAgent: '  /tmp/keepass-agent.sock  ',
    };
    expect(buildConnectionConfig(form).ssh_tunnel?.auth).toEqual({
      Agent: { identity_agent: '/tmp/keepass-agent.sock' },
    });
    expect(buildSaveConnectionInput(form, 'conn-1').ssh_tunnel).toMatchObject({
      identity_agent: '/tmp/keepass-agent.sock',
    });
  });

  it('keeps key auth free of agent fields', () => {
    const form = { ...sshForm, sshKeyPath: '~/.ssh/id_ed25519' };
    expect(buildConnectionConfig(form).ssh_tunnel?.auth).toEqual({
      Key: { private_key_path: '~/.ssh/id_ed25519', passphrase: undefined },
    });
    expect(buildSaveConnectionInput(form, 'conn-1').ssh_tunnel).toMatchObject({
      auth_type: 'key',
      key_path: '~/.ssh/id_ed25519',
      identity_agent: undefined,
    });
  });
});
