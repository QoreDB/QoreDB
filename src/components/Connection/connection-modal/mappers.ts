// SPDX-License-Identifier: Apache-2.0

import { isDocumentDatabase } from '@/lib/connection/driverCapabilities';
import { Driver, isKeyValueDriver } from '@/lib/connection/drivers';
import { normalizeMasking } from '@/lib/masking';
import type { ConnectionConfig, Environment, SavedConnection } from '@/lib/tauri';
import type { ConnectionFormData } from './types';

/** Search engines (Elasticsearch / OpenSearch) that carry a `search_auth_mode`. */
function isSearchDriver(driver: Driver): boolean {
  return driver === Driver.Elasticsearch || driver === Driver.OpenSearch;
}

function isSqlServerDriver(driver: Driver): boolean {
  return [Driver.SqlServer, Driver.AzureSql, Driver.Synapse].includes(driver);
}

/** The cloud warehouses have no dedicated config fields: their context rides in `options`. */
function driverOptions(formData: ConnectionFormData): Record<string, string> | undefined {
  const options = { ...formData.options };
  const optional: [string, string][] =
    formData.driver === Driver.Snowflake
      ? [
          ['warehouse', formData.snowflakeWarehouse],
          ['role', formData.snowflakeRole],
        ]
      : formData.driver === Driver.BigQuery
        ? [
            ['location', formData.bigqueryLocation],
            ['billing_project', formData.bigqueryBillingProject],
          ]
        : [];
  if (formData.driver === Driver.Snowflake) options.auth = formData.snowflakeAuthMode;
  for (const [key, value] of optional) {
    if (value.trim()) options[key] = value.trim();
    else delete options[key];
  }
  return Object.keys(options).length > 0 ? options : undefined;
}

/** BigQuery has no host to type: the API endpoint is fixed. */
function hostFor(formData: ConnectionFormData): string {
  return formData.driver === Driver.BigQuery ? 'bigquery.googleapis.com' : formData.host;
}

export function buildConnectionConfig(formData: ConnectionFormData): ConnectionConfig {
  return {
    driver: formData.driver,
    host: hostFor(formData),
    port: formData.port,
    username: formData.username,
    password: formData.password,
    database: formData.database || undefined,
    ssl: formData.ssl,
    ssl_mode: formData.sslMode || undefined,
    mssql_auth: isSqlServerDriver(formData.driver) ? formData.mssqlAuthMode : undefined,
    clickhouse_cluster:
      formData.driver === Driver.Clickhouse && formData.clickhouseCluster.trim().length > 0
        ? formData.clickhouseCluster.trim()
        : undefined,
    search_auth_mode: isSearchDriver(formData.driver) ? formData.searchAuthMode : undefined,
    ssl_ca_cert: formData.sslCaCert.trim() || undefined,
    options: driverOptions(formData),
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
    expose_to_agents: formData.exposeToAgents,
    masking: normalizeMasking(formData.masking),
    host: hostFor(formData),
    port: formData.port,
    username: formData.username,
    database: formData.database || undefined,
    ssl: formData.ssl,
    ssl_mode: formData.sslMode || undefined,
    mssql_auth: isSqlServerDriver(formData.driver) ? formData.mssqlAuthMode : undefined,
    clickhouse_cluster:
      formData.driver === Driver.Clickhouse && formData.clickhouseCluster.trim().length > 0
        ? formData.clickhouseCluster.trim()
        : undefined,
    search_auth_mode: isSearchDriver(formData.driver) ? formData.searchAuthMode : undefined,
    ssl_ca_cert: formData.sslCaCert.trim() || undefined,
    options: driverOptions(formData),
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
  const isSnowflake = formData.driver === Driver.Snowflake;
  const isBigQuery = formData.driver === Driver.BigQuery;
  // A Snowflake access token identifies the user by itself; so does a
  // BigQuery service account.
  const snowflakeNeedsUser = isSnowflake && formData.snowflakeAuthMode === 'key_pair';
  const authRequired =
    !isDocumentDatabase(formData.driver) &&
    !isKeyValueDriver(formData.driver) &&
    formData.driver !== Driver.Cassandra &&
    formData.driver !== Driver.ScyllaDb &&
    (!isSearchDriver(formData.driver) || searchNeedsUser) &&
    (!isSnowflake || snowflakeNeedsUser) &&
    !isBigQuery;
  // SQLite and DuckDB are file-based: only the file path (stored in host) matters
  const isFileBased = formData.driver === Driver.Sqlite || formData.driver === Driver.Duckdb;

  if (isFileBased) {
    if (!formData.host) missing.push('connection.filePath');
  } else if (isBigQuery) {
    if (!formData.password.trim()) missing.push('connection.bigquery.serviceAccount');
  } else {
    if (!formData.host) missing.push('connection.host');
    if (!Number.isInteger(formData.port) || formData.port < 1 || formData.port > 65535) {
      missing.push('connection.port');
    }

    const isMssqlIntegrated =
      isSqlServerDriver(formData.driver) && formData.mssqlAuthMode === 'windows_integrated';
    if (authRequired && !isMssqlIntegrated && !formData.username) {
      missing.push('connection.username');
    }

    const ntlmUsernameOk =
      !isSqlServerDriver(formData.driver) ||
      formData.mssqlAuthMode !== 'windows_ntlm' ||
      formData.username.includes('\\') ||
      formData.username.includes('@');
    if (!ntlmUsernameOk) missing.push('connection.mssql.ntlmUsernameInvalid');

    if (isSnowflake && !formData.password.trim()) {
      missing.push(
        formData.snowflakeAuthMode === 'token'
          ? 'connection.snowflake.token'
          : 'connection.snowflake.privateKey'
      );
    }
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
