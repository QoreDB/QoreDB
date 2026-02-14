import type { ConnectionConfig, Environment, SavedConnection } from '@/lib/tauri';
import { Driver } from '@/lib/drivers';

import type { ConnectionFormData } from './types';

function getPathBasename(path: string): string {
  const normalized = path.replace(/\\/g, '/');
  const parts = normalized.split('/').filter(Boolean);
  return parts.length ? parts[parts.length - 1] : path;
}

export function buildConnectionConfig(formData: ConnectionFormData): ConnectionConfig {
  return {
    driver: formData.driver,
    host: formData.host,
    port: formData.port,
    username: formData.username,
    password: formData.password,
    database: formData.database || undefined,
    ssl: formData.ssl,
    pool_max_connections: formData.poolMaxConnections,
    pool_min_connections: formData.poolMinConnections,
    pool_acquire_timeout_secs: formData.poolAcquireTimeoutSecs,
    environment: formData.environment,
    read_only: formData.readOnly,
    ssh_tunnel: formData.useSshTunnel
      ? {
          host: formData.sshHost,
          port: formData.sshPort,
          username: formData.sshUsername,
          auth: {
            Key: {
              private_key_path: formData.sshKeyPath,
              passphrase: formData.sshPassphrase || undefined,
            },
          },
          host_key_policy: formData.sshHostKeyPolicy,
          proxy_jump: formData.sshProxyJump || undefined,
          connect_timeout_secs: formData.sshConnectTimeoutSecs,
          keepalive_interval_secs: formData.sshKeepaliveIntervalSecs,
          keepalive_count_max: formData.sshKeepaliveCountMax,
        }
      : undefined,
  };
}

export function buildSavedConnection(
  formData: ConnectionFormData,
  connectionId: string,
  projectId: string = 'default'
): SavedConnection {
  return {
    id: connectionId,
    name: formData.name || `${formData.host}:${formData.port}`,
    driver: formData.driver,
    environment: formData.environment as Environment,
    read_only: formData.readOnly,
    host: formData.host,
    port: formData.port,
    username: formData.username,
    database: formData.database || undefined,
    ssl: formData.ssl,
    pool_max_connections: formData.poolMaxConnections,
    pool_min_connections: formData.poolMinConnections,
    pool_acquire_timeout_secs: formData.poolAcquireTimeoutSecs,
    project_id: projectId,
    ssh_tunnel: formData.useSshTunnel
      ? {
          host: formData.sshHost,
          port: formData.sshPort,
          username: formData.sshUsername,
          auth_type: 'key',
          key_path: formData.sshKeyPath,
          host_key_policy: formData.sshHostKeyPolicy,
          proxy_jump: formData.sshProxyJump || undefined,
          connect_timeout_secs: formData.sshConnectTimeoutSecs,
          keepalive_interval_secs: formData.sshKeepaliveIntervalSecs,
          keepalive_count_max: formData.sshKeepaliveCountMax,
        }
      : undefined,
  };
}

export function buildSaveConnectionInput(
  formData: ConnectionFormData,
  connectionId: string,
  projectId: string = 'default'
) {
  const savedConnection = buildSavedConnection(formData, connectionId, projectId);

  return {
    ...savedConnection,
    password: formData.password,
    ssh_tunnel: formData.useSshTunnel
      ? {
          host: formData.sshHost,
          port: formData.sshPort,
          username: formData.sshUsername,
          auth_type: 'key',
          key_path: formData.sshKeyPath,
          key_passphrase: formData.sshPassphrase || undefined,
          host_key_policy: formData.sshHostKeyPolicy,
          proxy_jump: formData.sshProxyJump || undefined,
          connect_timeout_secs: formData.sshConnectTimeoutSecs,
          keepalive_interval_secs: formData.sshKeepaliveIntervalSecs,
          keepalive_count_max: formData.sshKeepaliveCountMax,
        }
      : undefined,
  };
}

export function getSshSummary(formData: ConnectionFormData): string {
  if (!formData.useSshTunnel) return '';
  const hostPart = formData.sshHost ? `${formData.sshHost}:${formData.sshPort || 22}` : '(host?)';
  const userPart = formData.sshUsername ? `${formData.sshUsername}@` : '';
  const keyPart = formData.sshKeyPath ? `key:${getPathBasename(formData.sshKeyPath)}` : 'key:?';
  const policyPart = formData.sshHostKeyPolicy;
  return `${userPart}${hostPart} · ${keyPart} · ${policyPart}`;
}

export function isConnectionFormValid(formData: ConnectionFormData): boolean {
  // MongoDB and Redis often run without authentication in dev mode
  const authRequired = formData.driver !== Driver.Mongodb && formData.driver !== Driver.Redis;
  // SQLite is file-based and doesn't need host/username/password in the traditional sense
  const isFileBased = formData.driver === Driver.Sqlite;

  if (isFileBased) {
    // SQLite only requires a file path (stored in host field)
    return Boolean(
      formData.host &&
      (!formData.useSshTunnel || (formData.sshHost && formData.sshUsername && formData.sshKeyPath))
    );
  }

  return Boolean(
    formData.host &&
    (formData.username || !authRequired) &&
    (!formData.useSshTunnel || (formData.sshHost && formData.sshUsername && formData.sshKeyPath))
  );
}

export function normalizePortForDriver(driver: Driver): number {
  if (driver === Driver.Postgres) return 5432;
  if (driver === Driver.Mysql) return 3306;
  if (driver === Driver.Mongodb) return 27017;
  if (driver === Driver.Redis) return 6379;
  if (driver === Driver.Sqlite) return 0;
  return 5432;
}
