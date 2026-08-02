// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo, useState } from 'react';
import { supportsConnectionUrl } from '@/lib/connection/connectionUrls';
import { DEFAULT_PORTS, Driver } from '@/lib/connection/drivers';
import { detectDriverFromDsn } from '@/lib/connection/dsnDetector';
import { resolveMotherDuckHost } from '@/lib/connection/motherduck';
import type { PartialConnectionConfig, SavedConnection } from '@/lib/tauri';
import { isConnectionFormValid } from './mappers';
import { type ConnectionFormData, initialConnectionFormData } from './types';

function mapDriverString(driver: string | undefined): Driver | undefined {
  if (!driver) return undefined;
  const normalized = driver.toLowerCase();
  switch (normalized) {
    case 'postgres':
    case 'postgresql':
      return Driver.Postgres;
    case 'mysql':
      return Driver.Mysql;
    case 'mariadb':
      return Driver.Mariadb;
    case 'mongodb':
      return Driver.Mongodb;
    case 'redis':
      return Driver.Redis;
    case 'valkey':
      return Driver.Valkey;
    case 'sqlite':
    case 'sqlite3':
      return Driver.Sqlite;
    case 'duckdb':
      return Driver.Duckdb;
    case 'motherduck':
      return Driver.Motherduck;
    case 'sqlserver':
    case 'mssql':
      return Driver.SqlServer;
    case 'cockroachdb':
    case 'cockroach':
      return Driver.Cockroachdb;
    case 'timescaledb':
    case 'timescale':
      return Driver.Timescaledb;
    case 'clickhouse':
      return Driver.Clickhouse;
    default:
      return undefined;
  }
}

/**
 * Canonical SSL mode from a parsed URL. MySQL spells it `ssl-mode` with
 * uppercase values, PostgreSQL `sslmode` with lowercase ones.
 */
function normalizeSslMode(options: Record<string, string> | undefined): string | undefined {
  const raw = options?.['ssl-mode'] ?? options?.sslmode;
  if (!raw) return undefined;

  switch (raw.trim().toLowerCase()) {
    case 'disabled':
    case 'disable':
      return 'disable';
    case 'preferred':
    case 'prefer':
    case 'allow':
      return 'prefer';
    case 'required':
    case 'require':
      return 'require';
    case 'verify_ca':
    case 'verify-ca':
      return 'verify-ca';
    case 'verify_identity':
    case 'verify-identity':
    case 'verify-full':
      return 'verify-full';
    default:
      return undefined;
  }
}

function preserveCompatibleSelectedDriver(selected: Driver, parsed: Driver): Driver {
  if (
    parsed === Driver.Postgres &&
    [Driver.Supabase, Driver.Neon, Driver.Timescaledb, Driver.Motherduck].includes(selected)
  ) {
    return selected;
  }

  if (parsed === Driver.Mysql && selected === Driver.Mariadb) {
    return selected;
  }

  return parsed;
}

export function useConnectionForm(options: {
  isOpen: boolean;
  editConnection?: SavedConnection;
  editPassword?: string;
}) {
  const { isOpen, editConnection, editPassword } = options;
  const [formData, setFormData] = useState<ConnectionFormData>(initialConnectionFormData);

  useEffect(() => {
    if (!isOpen) return;

    if (editConnection) {
      const sshTunnel = editConnection.ssh_tunnel;
      const proxy = editConnection.proxy;
      const driver = editConnection.driver as Driver;
      setFormData({
        name: editConnection.name,
        driver,
        environment: editConnection.environment || 'development',
        readOnly: editConnection.read_only || false,
        host:
          driver === Driver.Motherduck
            ? resolveMotherDuckHost(editConnection.host, editPassword || '')
            : editConnection.host,
        port: editConnection.port,
        username: editConnection.username,
        password: editPassword || '',
        database: editConnection.database || '',
        ssl: editConnection.ssl,
        sslMode: editConnection.ssl_mode || '',
        mssqlAuthMode: editConnection.mssql_auth ?? 'sql_password',
        clickhouseCluster: editConnection.clickhouse_cluster ?? '',
        searchAuthMode: editConnection.search_auth_mode ?? 'none',
        sslCaCert: editConnection.ssl_ca_cert ?? '',
        poolMaxConnections: editConnection.pool_max_connections ?? 5,
        poolMinConnections: editConnection.pool_min_connections ?? 0,
        poolAcquireTimeoutSecs: editConnection.pool_acquire_timeout_secs ?? 30,
        useSshTunnel: !!sshTunnel,
        sshHost: sshTunnel ? sshTunnel.host : '',
        sshPort: sshTunnel ? sshTunnel.port : 22,
        sshUsername: sshTunnel ? sshTunnel.username : '',
        sshKeyPath: sshTunnel ? sshTunnel.key_path || '' : '',
        sshHostKeyPolicy: sshTunnel
          ? (sshTunnel.host_key_policy as ConnectionFormData['sshHostKeyPolicy'])
          : 'accept_new',
        sshProxyJump: sshTunnel ? sshTunnel.proxy_jump || '' : '',
        sshConnectTimeoutSecs: sshTunnel ? sshTunnel.connect_timeout_secs : 10,
        sshKeepaliveIntervalSecs: sshTunnel ? sshTunnel.keepalive_interval_secs : 30,
        sshKeepaliveCountMax: sshTunnel ? sshTunnel.keepalive_count_max : 3,
        useProxy: !!proxy,
        proxyType: proxy ? (proxy.proxy_type as ConnectionFormData['proxyType']) : 'socks5',
        proxyHost: proxy ? proxy.host : '',
        proxyPort: proxy ? proxy.port : 1080,
        proxyUsername: proxy ? proxy.username || '' : '',
        proxyPassword: '',
        proxyConnectTimeoutSecs: proxy ? proxy.connect_timeout_secs : 10,
        useUrl: false,
        connectionUrl: '',
        options: editConnection.options ?? {},
      });
    } else {
      setFormData(initialConnectionFormData);
    }
  }, [isOpen, editConnection, editPassword]);

  function handleDriverChange(driver: Driver) {
    setFormData(prev => ({
      ...prev,
      driver,
      port: DEFAULT_PORTS[driver],
      host:
        driver === Driver.Motherduck && prev.host === 'localhost'
          ? resolveMotherDuckHost('', prev.password)
          : prev.host,
      username: driver === Driver.Motherduck && !prev.username ? 'postgres' : prev.username,
      database: driver === Driver.Motherduck && !prev.database ? 'md:' : prev.database,
      ssl: driver === Driver.Motherduck ? true : prev.ssl,
      sslMode: driver === Driver.Motherduck && !prev.sslMode ? 'verify-full' : prev.sslMode,
      // Cloud-managed Postgres providers are almost always configured via DSN —
      // pre-enable the URL toggle so the user can paste right away.
      useUrl: driverPrefersUrl(driver) ? true : supportsConnectionUrl(driver) ? prev.useUrl : false,
      connectionUrl: driver === prev.driver ? prev.connectionUrl : '',
    }));
  }

  function driverPrefersUrl(driver: Driver): boolean {
    return driver === Driver.Supabase || driver === Driver.Neon || driver === Driver.Motherduck;
  }

  function handleChange(field: keyof ConnectionFormData, value: string | number | boolean) {
    setFormData(prev => {
      const next = { ...prev, [field]: value };
      if (prev.driver === Driver.Motherduck && field === 'password' && typeof value === 'string') {
        next.host = resolveMotherDuckHost(prev.host, value);
      }
      return next;
    });
  }

  /**
   * Apply parsed URL configuration to form fields.
   * URL-derived values are applied, but existing non-empty values for name,
   * environment, readOnly, and pool settings are preserved (user overrides).
   *
   * `rawUrl` is the original DSN — used to detect specialized cloud drivers
   * (Supabase, Neon) that share the `postgres://` scheme with vanilla Postgres
   * but resolve to a specific managed provider. Detection wins over the
   * scheme-based mapping so the connection keeps its distinct driver/icon.
   */
  const applyParsedConfig = useCallback((config: PartialConnectionConfig, rawUrl?: string) => {
    setFormData(prev => {
      const detected = rawUrl ? detectDriverFromDsn(rawUrl) : null;
      const parsedDriver = mapDriverString(config.driver);
      const driver =
        detected?.driver ??
        (parsedDriver ? preserveCompatibleSelectedDriver(prev.driver, parsedDriver) : prev.driver);
      const port = config.port ?? DEFAULT_PORTS[driver];
      const password = config.password ?? prev.password;
      const host = config.host ?? prev.host;

      return {
        ...prev,
        // Apply URL-derived values
        driver,
        host: driver === Driver.Motherduck ? resolveMotherDuckHost(host, password) : host,
        port,
        username:
          config.username ??
          (driver === Driver.Motherduck && !prev.username ? 'postgres' : prev.username),
        password,
        database:
          config.database ??
          (driver === Driver.Motherduck && !prev.database ? 'md:' : prev.database),
        ssl: config.ssl ?? (driver === Driver.Motherduck ? true : prev.ssl),
        sslMode:
          normalizeSslMode(config.options) ??
          (driver === Driver.Motherduck && !prev.sslMode ? 'verify-full' : prev.sslMode),
        options: config.options ?? prev.options,
      };
    });
  }, []);

  const isValid = useMemo(() => isConnectionFormValid(formData), [formData]);

  return {
    formData,
    setFormData,
    handleDriverChange,
    handleChange,
    applyParsedConfig,
    isValid,
  };
}
