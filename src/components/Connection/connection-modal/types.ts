// SPDX-License-Identifier: Apache-2.0

import { Driver } from '@/lib/connection/drivers';
import { EMPTY_MASKING } from '@/lib/masking';
import type {
  ConnectionMasking,
  Environment,
  MssqlAuthMode,
  SearchAuthMode,
  SnowflakeAuthMode,
} from '@/lib/tauri';

export interface ConnectionFormData {
  name: string;
  driver: Driver;
  environment: Environment;
  readOnly: boolean;
  exposeToAgents: boolean;
  host: string;
  port: number;
  username: string;
  password: string;
  database: string;
  ssl: boolean;
  sslMode: string;
  mssqlAuthMode: MssqlAuthMode;
  /** ClickHouse distributed cluster name. Empty string = no `ON CLUSTER`. */
  clickhouseCluster: string;
  /** Auth mode for Elasticsearch / OpenSearch. */
  searchAuthMode: SearchAuthMode;
  /** Snowflake: key pair signs a JWT, token is a programmatic access token. */
  snowflakeAuthMode: SnowflakeAuthMode;
  snowflakeWarehouse: string;
  snowflakeRole: string;
  /** BigQuery: dataset location and the project billed for queries. */
  bigqueryLocation: string;
  bigqueryBillingProject: string;
  /** Path to a custom CA certificate (PEM) for TLS verification. */
  sslCaCert: string;
  poolMaxConnections: number;
  poolMinConnections: number;
  poolAcquireTimeoutSecs: number;
  useSshTunnel: boolean;
  sshHost: string;
  sshPort: number;
  sshUsername: string;
  sshKeyPath: string;
  sshHostKeyPolicy: 'accept_new' | 'strict' | 'insecure_no_check';
  sshProxyJump: string;
  sshConnectTimeoutSecs: number;
  sshKeepaliveIntervalSecs: number;
  sshKeepaliveCountMax: number;
  useProxy: boolean;
  proxyType: 'http_connect' | 'socks5';
  proxyHost: string;
  proxyPort: number;
  proxyUsername: string;
  proxyPassword: string;
  proxyConnectTimeoutSecs: number;
  useUrl: boolean;
  connectionUrl: string;
  /** Driver options carried over from a parsed URL, plus the warehouse fields above. */
  options: Record<string, string>;
  masking: ConnectionMasking;
}

export const initialConnectionFormData: ConnectionFormData = {
  name: '',
  driver: Driver.Postgres,
  environment: 'development',
  readOnly: false,
  exposeToAgents: false,
  host: 'localhost',
  port: 5432,
  username: '',
  password: '',
  database: '',
  ssl: false,
  sslMode: '',
  mssqlAuthMode: 'sql_password',
  clickhouseCluster: '',
  searchAuthMode: 'none',
  snowflakeAuthMode: 'key_pair',
  snowflakeWarehouse: '',
  snowflakeRole: '',
  bigqueryLocation: '',
  bigqueryBillingProject: '',
  sslCaCert: '',
  poolMaxConnections: 5,
  poolMinConnections: 0,
  poolAcquireTimeoutSecs: 30,
  useSshTunnel: false,
  sshHost: '',
  sshPort: 22,
  sshUsername: '',
  sshKeyPath: '',
  sshHostKeyPolicy: 'accept_new',
  sshProxyJump: '',
  sshConnectTimeoutSecs: 10,
  sshKeepaliveIntervalSecs: 30,
  sshKeepaliveCountMax: 3,
  useProxy: false,
  proxyType: 'socks5',
  proxyHost: '',
  proxyPort: 1080,
  proxyUsername: '',
  proxyPassword: '',
  proxyConnectTimeoutSecs: 10,
  useUrl: false,
  connectionUrl: '',
  options: {},
  masking: EMPTY_MASKING,
};
