/**
 * Tauri API wrappers for type-safe invocations
 */
import { invoke } from '@tauri-apps/api/core';

// ============================================
// TYPES
// ============================================

export type Environment = 'development' | 'staging' | 'production';

export interface ConnectionConfig {
  driver: string;
  host: string;
  port: number;
  username: string;
  password: string;
  database?: string;
  ssl: boolean;
  environment: Environment;
  read_only: boolean;
  pool_max_connections?: number;
  pool_min_connections?: number;
  pool_acquire_timeout_secs?: number;
  ssh_tunnel?: SshTunnelConfig;
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
  pool_max_connections?: number;
  pool_min_connections?: number;
  pool_acquire_timeout_secs?: number;
  project_id: string;
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
  collection_type: 'Table' | 'View' | 'Collection';
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

// ============================================
// CONNECTION COMMANDS
// ============================================

export async function testConnection(config: ConnectionConfig): Promise<ConnectionResponse> {
  return invoke('test_connection', { config });
}

export async function testSavedConnection(
  projectId: string,
  connectionId: string
): Promise<ConnectionResponse> {
  return invoke('test_saved_connection', { projectId, connectionId });
}

export async function connect(config: ConnectionConfig): Promise<ConnectionResponse> {
  return invoke('connect', { config });
}

export async function connectSavedConnection(
  projectId: string,
  connectionId: string
): Promise<ConnectionResponse> {
  return invoke('connect_saved_connection', { projectId, connectionId });
}

export async function disconnect(sessionId: string): Promise<ConnectionResponse> {
  return invoke('disconnect', { sessionId });
}

export async function listSessions(): Promise<SessionListItem[]> {
  return invoke('list_sessions');
}

// ============================================
// POLICY COMMANDS
// ============================================

export async function getSafetyPolicy(): Promise<SafetyPolicyResponse> {
  return invoke('get_safety_policy');
}

export async function setSafetyPolicy(policy: SafetyPolicy): Promise<SafetyPolicyResponse> {
  return invoke('set_safety_policy', { policy });
}

// ============================================
// QUERY COMMANDS
// ============================================

export async function executeQuery(
  sessionId: string,
  query: string,
  options?: {
    acknowledgedDangerous?: boolean;
    timeoutMs?: number;
    stream?: boolean;
    queryId?: string;
    namespace?: Namespace;
  }
): Promise<{
  success: boolean;
  result?: QueryResult;
  error?: string;
  query_id?: string;
}> {
  return invoke('execute_query', {
    sessionId,
    query,
    namespace: options?.namespace,
    acknowledgedDangerous: options?.acknowledgedDangerous,
    queryId: options?.queryId,
    timeoutMs: options?.timeoutMs,
    stream: options?.stream,
  });
}

export async function listNamespaces(sessionId: string): Promise<{
  success: boolean;
  namespaces?: Namespace[];
  error?: string;
}> {
  return invoke('list_namespaces', { sessionId });
}

export async function listCollections(
  sessionId: string,
  namespace: Namespace,
  search?: string,
  page?: number,
  page_size?: number
): Promise<{
  success: boolean;
  data?: CollectionList;
  error?: string;
}> {
  return invoke('list_collections', { sessionId, namespace, search, page, page_size });
}

export async function cancelQuery(
  sessionId: string,
  queryId?: string
): Promise<{
  success: boolean;
  error?: string;
  query_id?: string;
}> {
  return invoke('cancel_query', { sessionId, queryId });
}

export async function createDatabase(
  sessionId: string,
  name: string,
  options?: Record<string, unknown>
): Promise<{
  success: boolean;
  error?: string;
}> {
  return invoke('create_database', { sessionId, name, options });
}

export async function dropDatabase(
  sessionId: string,
  name: string
): Promise<{
  success: boolean;
  error?: string;
}> {
  return invoke('drop_database', { sessionId, name });
}

// ============================================
// TABLE BROWSING
// ============================================

export interface ForeignKey {
  column: string;
  referenced_table: string;
  referenced_column: string;
  referenced_schema?: string;
  referenced_database?: string;
  constraint_name?: string;
}

export interface RelationFilter {
  foreignKey: ForeignKey;
  value: Value;
}

export interface TableIndex {
  name: string;
  columns: string[];
  is_unique: boolean;
  is_primary: boolean;
}

export interface TableSchema {
  columns: TableColumn[];
  primary_key?: string[];
  foreign_keys: ForeignKey[];
  row_count_estimate?: number | null;
  indexes: TableIndex[];
}

export interface TableColumn {
  name: string;
  data_type: string;
  nullable: boolean;
  default_value?: string;
  is_primary_key: boolean;
}

export type CancelSupport = 'none' | 'best_effort' | 'driver';

export interface DriverCapabilities {
  transactions: boolean;
  mutations: boolean;
  cancel: CancelSupport;
  supports_ssh: boolean;
  schema: boolean;
  streaming: boolean;
  explain: boolean;
}

export interface DriverInfo {
  id: string;
  name: string;
  capabilities: DriverCapabilities;
}

export async function describeTable(
  sessionId: string,
  namespace: Namespace,
  table: string
): Promise<{
  success: boolean;
  schema?: TableSchema;
  error?: string;
}> {
  return invoke('describe_table', { sessionId, namespace, table });
}

export async function previewTable(
  sessionId: string,
  namespace: Namespace,
  table: string,
  limit: number = 100
): Promise<{
  success: boolean;
  result?: QueryResult;
  error?: string;
}> {
  return invoke('preview_table', { sessionId, namespace, table, limit });
}

export async function peekForeignKey(
  sessionId: string,
  namespace: Namespace,
  foreignKey: ForeignKey,
  value: Value,
  limit: number = 3
): Promise<{
  success: boolean;
  result?: QueryResult;
  error?: string;
}> {
  return invoke('peek_foreign_key', { sessionId, namespace, foreignKey, value, limit });
}

// ============================================
// DRIVER METADATA
// ============================================

export async function getDriverInfo(sessionId: string): Promise<{
  success: boolean;
  driver?: DriverInfo;
  error?: string;
}> {
  return invoke('get_driver_info', { sessionId });
}

export async function listDrivers(): Promise<{
  success: boolean;
  drivers: DriverInfo[];
  error?: string;
}> {
  return invoke('list_drivers');
}

// ============================================
// TRANSACTIONS
// ============================================

export async function beginTransaction(sessionId: string): Promise<{
  success: boolean;
  error?: string;
}> {
  return invoke('begin_transaction', { sessionId });
}

export async function commitTransaction(sessionId: string): Promise<{
  success: boolean;
  error?: string;
}> {
  return invoke('commit_transaction', { sessionId });
}

export async function rollbackTransaction(sessionId: string): Promise<{
  success: boolean;
  error?: string;
}> {
  return invoke('rollback_transaction', { sessionId });
}

export async function supportsTransactions(sessionId: string): Promise<boolean> {
  return invoke('supports_transactions', { sessionId });
}

// ============================================
// MUTATIONS
// ============================================

export interface RowData {
  columns: Record<string, Value>;
}

export interface MutationResponse {
  success: boolean;
  result?: QueryResult;
  error?: string;
}

export async function insertRow(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  table: string,
  data: RowData
): Promise<MutationResponse> {
  return invoke('insert_row', { sessionId, database, schema, table, data });
}

export async function updateRow(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  table: string,
  primaryKey: RowData,
  data: RowData
): Promise<MutationResponse> {
  return invoke('update_row', {
    sessionId,
    database,
    schema,
    table,
    primaryKey,
    data,
  });
}

export async function deleteRow(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  table: string,
  primaryKey: RowData
): Promise<MutationResponse> {
  return invoke('delete_row', {
    sessionId,
    database,
    schema,
    table,
    primaryKey,
  });
}

export async function supportsMutations(sessionId: string): Promise<boolean> {
  return invoke('supports_mutations', { sessionId });
}

// ============================================
// LOGS
// ============================================

export async function exportLogs(): Promise<{
  success: boolean;
  filename?: string;
  content?: string;
  error?: string;
}> {
  return invoke('export_logs');
}

export async function getMetrics(): Promise<{
  success: boolean;
  metrics?: {
    total: number;
    failed: number;
    cancelled: number;
    timeouts: number;
    avg_ms?: number;
    max_ms?: number;
  };
  error?: string;
}> {
  return invoke('get_metrics');
}

// ============================================

export async function getVaultStatus(): Promise<VaultStatus> {
  return invoke('get_vault_status');
}

export async function setupMasterPassword(password: string): Promise<VaultResponse> {
  return invoke('setup_master_password', { password });
}

export async function unlockVault(password: string): Promise<VaultResponse> {
  return invoke('unlock_vault', { password });
}

export async function lockVault(): Promise<VaultResponse> {
  return invoke('lock_vault');
}

export async function saveConnection(input: {
  id: string;
  name: string;
  driver: string;
  environment: Environment;
  read_only: boolean;
  host: string;
  port: number;
  username: string;
  password: string;
  database?: string;
  ssl: boolean;
  pool_max_connections?: number;
  pool_min_connections?: number;
  pool_acquire_timeout_secs?: number;
  project_id: string;
  ssh_tunnel?: {
    host: string;
    port: number;
    username: string;
    auth_type: string;
    password?: string;
    key_path?: string;
    key_passphrase?: string;

    host_key_policy: string;
    proxy_jump?: string;
    connect_timeout_secs: number;
    keepalive_interval_secs: number;
    keepalive_count_max: number;
  };
}): Promise<VaultResponse> {
  return invoke('save_connection', { input });
}

export async function listSavedConnections(projectId: string): Promise<SavedConnection[]> {
  return invoke('list_saved_connections', { projectId });
}

export async function getConnectionCredentials(projectId: string, connectionId: string): Promise<{
  success: boolean;
  password?: string;
  error?: string;
}> {
  return invoke('get_connection_credentials', { projectId, connectionId });
}

export async function deleteSavedConnection(projectId: string, connectionId: string): Promise<VaultResponse> {
  return invoke('delete_saved_connection', { projectId, connectionId });
}

export async function duplicateSavedConnection(
  projectId: string,
  connectionId: string
): Promise<{
  success: boolean;
  connection?: SavedConnection;
  error?: string;
}> {
  return invoke('duplicate_saved_connection', { projectId, connectionId });
}

// ============================================
// SANDBOX COMMANDS
// ============================================

export type SandboxChangeType = 'insert' | 'update' | 'delete';

export interface SandboxChangeDto {
  change_type: SandboxChangeType;
  namespace: Namespace;
  table_name: string;
  primary_key?: RowData;
  old_values?: Record<string, Value>;
  new_values?: Record<string, Value>;
}

export interface MigrationScript {
  sql: string;
  statement_count: number;
  warnings: string[];
}

export interface FailedChange {
  index: number;
  error: string;
}

export interface ApplySandboxResult {
  success: boolean;
  applied_count: number;
  error?: string;
  failed_changes: FailedChange[];
}

export async function generateMigrationSql(
  sessionId: string,
  changes: SandboxChangeDto[]
): Promise<{
  success: boolean;
  script?: MigrationScript;
  error?: string;
}> {
  return invoke('generate_migration_sql', { sessionId, changes });
}

export async function applySandboxChanges(
  sessionId: string,
  changes: SandboxChangeDto[],
  useTransaction: boolean = true
): Promise<ApplySandboxResult> {
  return invoke('apply_sandbox_changes', { sessionId, changes, useTransaction });
}

// ============================================
// FULL-TEXT SEARCH
// ============================================

export interface FulltextMatch {
  namespace: Namespace;
  table_name: string;
  column_name: string;
  value_preview: string;
  row_preview: [string, Value][];
}

export interface SearchFilter {
  column: string;
  value: string;
  caseSensitive?: boolean;
}

export interface FulltextSearchOptions {
  max_results_per_table?: number;
  max_total_results?: number;
  case_sensitive?: boolean;
  namespaces?: Namespace[];
  tables?: string[];
}

export interface SearchStats {
  native_fulltext_count: number;
  pattern_match_count: number;
  timeout_count: number;
  error_count: number;
}

export interface FulltextSearchResponse {
  success: boolean;
  matches: FulltextMatch[];
  total_matches: number;
  tables_searched: number;
  search_time_ms: number;
  error?: string;
  truncated: boolean;
  stats: SearchStats;
}

export async function fulltextSearch(
  sessionId: string,
  searchTerm: string,
  options?: FulltextSearchOptions
): Promise<FulltextSearchResponse> {
  return invoke('fulltext_search', { sessionId, searchTerm, options });
}
