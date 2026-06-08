// SPDX-License-Identifier: Apache-2.0

export type Environment = 'development' | 'staging' | 'production';

export type MssqlAuthMode = 'sql_password' | 'windows_ntlm' | 'windows_integrated';

export interface ConnectionConfig {
  driver: string;
  host: string;
  port: number;
  username: string;
  password: string;
  database?: string;
  ssl: boolean;
  ssl_mode?: string;
  environment: Environment;
  read_only: boolean;
  pool_max_connections?: number;
  pool_min_connections?: number;
  pool_acquire_timeout_secs?: number;
  ssh_tunnel?: SshTunnelConfig;
  proxy?: ProxyConfig;
  mssql_auth?: MssqlAuthMode;
  /** Distributed cluster name for ClickHouse DDL (`ON CLUSTER`). */
  clickhouse_cluster?: string;
}

export type ProxyType = 'http_connect' | 'socks5';

export interface ProxyConfig {
  proxy_type: ProxyType;
  host: string;
  port: number;
  username?: string;
  password?: string;
  connect_timeout_secs: number;
}

export interface SshTunnelConfig {
  host: string;
  port: number;
  username: string;
  auth: SshAuth;

  /** Security-critical host key policy */
  host_key_policy: 'accept_new' | 'strict' | 'insecure_no_check';

  /** Optional bastion/jump host, e.g. user@bastion:22 */
  proxy_jump?: string;
  connect_timeout_secs: number;
  keepalive_interval_secs: number;
  keepalive_count_max: number;
}

export type SshAuth =
  | { Password: { password: string } }
  | { Key: { private_key_path: string; passphrase?: string } };

export interface ConnectionResponse {
  success: boolean;
  session_id?: string;
  error?: string;
}

export interface SessionListItem {
  id: string;
  display_name: string;
}

export interface SavedConnection {
  id: string;
  name: string;
  driver: string;
  environment: Environment;
  read_only: boolean;
  host: string;
  port: number;
  username: string;
  database?: string;
  ssl: boolean;
  ssl_mode?: string;
  pool_max_connections?: number;
  pool_min_connections?: number;
  pool_acquire_timeout_secs?: number;
  project_id: string;
  mssql_auth?: MssqlAuthMode;
  /** Distributed cluster name for ClickHouse DDL (`ON CLUSTER`). */
  clickhouse_cluster?: string;
  ssh_tunnel?: {
    host: string;
    port: number;
    username: string;
    auth_type: string;
    key_path?: string;

    host_key_policy: string;
    proxy_jump?: string;
    connect_timeout_secs: number;
    keepalive_interval_secs: number;
    keepalive_count_max: number;
  };
  proxy?: {
    proxy_type: string;
    host: string;
    port: number;
    username?: string;
    connect_timeout_secs: number;
  };
}

export interface VaultStatus {
  is_locked: boolean;
  has_master_password: boolean;
}

export interface VaultResponse {
  success: boolean;
  error?: string;
}

export interface SafetyPolicy {
  prod_require_confirmation: boolean;
  prod_block_dangerous_sql: boolean;
  query_rate_limit_enabled?: boolean;
}

export interface SafetyPolicyResponse {
  success: boolean;
  policy?: SafetyPolicy;
  error?: string;
}

export interface Namespace {
  database: string;
  schema?: string;
}

export interface Collection {
  namespace: Namespace;
  name: string;
  collection_type: 'Table' | 'View' | 'MaterializedView' | 'Collection';
}

export interface CollectionListOptions {
  search?: string;
  page?: number;
  page_size?: number;
}

export interface CollectionList {
  collections: Collection[];
  total_count: number;
}

export interface QueryResult {
  columns: ColumnInfo[];
  rows: Row[];
  affected_rows?: number;
  execution_time_ms: number;
  total_time_ms?: number;
}

export interface ColumnInfo {
  name: string;
  data_type: string;
  nullable: boolean;
}

export type Row = { values: Value[] };
export type Value = null | boolean | number | string | object;
