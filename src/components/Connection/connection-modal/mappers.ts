// SPDX-License-Identifier: Apache-2.0

import { Driver } from '@/lib/connection/drivers';
import type { ConnectionConfig, Environment, SavedConnection } from '@/lib/tauri';

import type { ConnectionFormData } from './types';

/** Search engines (Elasticsearch / OpenSearch) that carry a `search_auth_mode`. */
function isSearchDriver(driver: Driver): boolean {
  return driver === Driver.Elasticsearch || driver === Driver.OpenSearch;
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
    ssl_mode: formData.sslMode || undefined,
    mssql_auth: formData.driver === Driver.SqlServer ? formData.mssqlAuthMode : undefined,
    clickhouse_cluster:
      formData.driver === Driver.Clickhouse && formData.clickhouseCluster.trim().length > 0
        ? formData.clickhouseCluster.trim()
        : undefined,
    search_auth_mode: isSearchDriver(formData.driver) ? formData.searchAuthMode : undefined,
    ssl_ca_cert: formData.sslCaCert.trim() || undefined,
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
              passphrase: undefined,
            },
          },
          host_key_policy: formData.sshHostKeyPolicy,
          proxy_jump: formData.sshProxyJump || undefined,
          connect_timeout_secs: formData.sshConnectTimeoutSecs,
          keepalive_interval_secs: formData.sshKeepaliveIntervalSecs,
          keepalive_count_max: formData.sshKeepaliveCountMax,
        }
      : undefined,
    proxy: formData.useProxy
      ? {
          proxy_type: formData.proxyType,
          host: formData.proxyHost,
          port: formData.proxyPort,
          username: formData.proxyUsername || undefined,
          password: formData.proxyPassword || undefined,
          connect_timeout_secs: formData.proxyConnectTimeoutSecs,
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
    ssl_mode: formData.sslMode || undefined,
    mssql_auth: formData.driver === Driver.SqlServer ? formData.mssqlAuthMode : undefined,
    clickhouse_cluster:
      formData.driver === Driver.Clickhouse && formData.clickhouseCluster.trim().length > 0
        ? formData.clickhouseCluster.trim()
        : undefined,
    search_auth_mode: isSearchDriver(formData.driver) ? formData.searchAuthMode : undefined,
    ssl_ca_cert: formData.sslCaCert.trim() || undefined,
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
    proxy: formData.useProxy
      ? {
          proxy_type: formData.proxyType,
          host: formData.proxyHost,
          port: formData.proxyPort,
          username: formData.proxyUsername || undefined,
          connect_timeout_secs: formData.proxyConnectTimeoutSecs,
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
          key_passphrase: undefined,
          host_key_policy: formData.sshHostKeyPolicy,
          proxy_jump: formData.sshProxyJump || undefined,
          connect_timeout_secs: formData.sshConnectTimeoutSecs,
          keepalive_interval_secs: formData.sshKeepaliveIntervalSecs,
          keepalive_count_max: formData.sshKeepaliveCountMax,
        }
      : undefined,
    proxy: formData.useProxy
      ? {
          proxy_type: formData.proxyType,
          host: formData.proxyHost,
          port: formData.proxyPort,
          username: formData.proxyUsername || undefined,
          password: formData.proxyPassword || undefined,
          connect_timeout_secs: formData.proxyConnectTimeoutSecs,
        }
      : undefined,
  };
}

/**
 * Returns the i18n keys of the requirements that are not yet satisfied.
 * An empty array means the form is ready to test/save. Used both to gate the
 * action buttons and to tell the user exactly what is missing.
 */
export function getMissingRequirements(formData: ConnectionFormData): string[] {
  const missing: string[] = [];

  // MongoDB and Redis often run without authentication in dev mode.
  // Search engines (ES/OS) only need a username in basic-auth mode.
  const searchNeedsUser = isSearchDriver(formData.driver) && formData.searchAuthMode === 'basic';
  const authRequired =
    formData.driver !== Driver.Mongodb &&
    formData.driver !== Driver.Redis &&
    (!isSearchDriver(formData.driver) || searchNeedsUser);
  // SQLite and DuckDB are file-based: only the file path (stored in host) matters
  const isFileBased = formData.driver === Driver.Sqlite || formData.driver === Driver.Duckdb;

  if (isFileBased) {
    if (!formData.host) missing.push('connection.filePath');
  } else {
    if (!formData.host) missing.push('connection.host');
    if (!Number.isInteger(formData.port) || formData.port < 1 || formData.port > 65535) {
      missing.push('connection.port');
    }

    const isMssqlIntegrated =
      formData.driver === Driver.SqlServer && formData.mssqlAuthMode === 'windows_integrated';
    if (authRequired && !isMssqlIntegrated && !formData.username) {
      missing.push('connection.username');
    }

    const ntlmUsernameOk =
      formData.driver !== Driver.SqlServer ||
      formData.mssqlAuthMode !== 'windows_ntlm' ||
      formData.username.includes('\\') ||
      formData.username.includes('@');
    if (!ntlmUsernameOk) missing.push('connection.mssql.ntlmUsernameInvalid');
  }

  if (formData.useSshTunnel) {
    if (!formData.sshHost) missing.push('connection.ssh.host');
    if (!formData.sshUsername) missing.push('connection.ssh.username');
    if (!formData.sshKeyPath) missing.push('connection.ssh.keyPath');
  }

  return missing;
}

export function isConnectionFormValid(formData: ConnectionFormData): boolean {
  return getMissingRequirements(formData).length === 0;
}
