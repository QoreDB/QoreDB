// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { Driver } from '@/lib/connection/drivers';
import {
  buildConnectionConfig,
  buildSaveConnectionInput,
  extractPgTlsFiles,
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

describe('PostgreSQL TLS client files', () => {
  const pgForm = {
    ...initialConnectionFormData,
    driver: Driver.Postgres,
    username: 'app',
    ssl: true,
    sslMode: 'verify-full',
    sslCaCert: ' /certs/ca.crt ',
    sslClientCert: '/certs/client.crt',
    sslClientKey: '/certs/client.key',
  };

  it('sends the three files to the backend and the vault', () => {
    const expected = {
      ssl_ca_cert: '/certs/ca.crt',
      ssl_client_cert: '/certs/client.crt',
      ssl_client_key: '/certs/client.key',
    };
    expect(buildConnectionConfig(pgForm)).toMatchObject(expected);
    expect(buildSaveConnectionInput(pgForm, 'id', 'project')).toMatchObject(expected);
  });

  it('requires the certificate and the key together', () => {
    expect(getMissingRequirements({ ...pgForm, sslClientKey: '' })).toEqual([
      'connection.tls.clientKey',
    ]);
    expect(getMissingRequirements({ ...pgForm, sslClientCert: '' })).toEqual([
      'connection.tls.clientCert',
    ]);
  });

  it('drops the client identity for drivers that do not use it', () => {
    const config = buildConnectionConfig({ ...pgForm, driver: Driver.Mysql });
    expect(config.ssl_client_cert).toBeUndefined();
    expect(config.ssl_client_key).toBeUndefined();
  });

  it('moves libpq URL parameters into the dedicated fields', () => {
    expect(
      extractPgTlsFiles(Driver.Postgres, {
        sslrootcert: '/certs/ca.crt',
        sslcert: '/certs/client.crt',
        application_name: 'qoredb',
      })
    ).toEqual({
      options: { application_name: 'qoredb' },
      files: { sslCaCert: '/certs/ca.crt', sslClientCert: '/certs/client.crt', sslClientKey: '' },
    });
    expect(extractPgTlsFiles(Driver.Mysql, { sslcert: 'x' })).toEqual({
      options: { sslcert: 'x' },
      files: {},
    });
  });
});

describe('SSH tunnel authentication', () => {
  const sshForm = {
    ...initialConnectionFormData,
    username: 'app',
    useSshTunnel: true,
    sshHost: 'bastion.example.com',
    sshUsername: 'ops',
  };

  it('uses the agent without a key file', () => {
    const form = { ...sshForm, sshAuthMethod: 'agent' as const, sshKeyPath: '/stale/id_rsa' };
    expect(getMissingRequirements(form)).toEqual([]);
    expect(buildConnectionConfig(form).ssh_tunnel?.auth).toEqual({ Agent: {} });
    expect(buildSaveConnectionInput(form, 'id', 'project').ssh_tunnel).toMatchObject({
      auth_type: 'agent',
      key_path: undefined,
    });
  });

  it('still requires a key path for key authentication', () => {
    expect(getMissingRequirements(sshForm)).toEqual(['connection.ssh.keyPath']);
    const form = { ...sshForm, sshKeyPath: '~/.ssh/id_ed25519' };
    expect(buildConnectionConfig(form).ssh_tunnel?.auth).toEqual({
      Key: { private_key_path: '~/.ssh/id_ed25519', passphrase: undefined },
    });
    expect(buildSaveConnectionInput(form, 'id', 'project').ssh_tunnel?.auth_type).toBe('key');
  });
});
