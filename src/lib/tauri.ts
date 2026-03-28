// SPDX-License-Identifier: Apache-2.0

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
  ssl_mode?: string;
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
  ssl_mode?: string;
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

// ============================================
// CONNECTION URL PARSING
// ============================================

export type ParseErrorCode =
  | 'invalid_url'
  | 'unsupported_scheme'
  | 'missing_host'
  | 'invalid_port'
  | 'invalid_utf8';

export interface PartialConnectionConfig {
  driver?: string;
  host?: string;
  port?: number;
  username?: string;
  password?: string;
  database?: string;
  ssl?: boolean;
  options: Record<string, string>;
}

export interface ParseConnectionUrlResponse {
  success: boolean;
  config?: PartialConnectionConfig;
  error?: string;
  error_code?: ParseErrorCode;
}

export async function parseConnectionUrl(url: string): Promise<ParseConnectionUrlResponse> {
  return invoke('parse_url', { url });
}

export async function getSupportedUrlSchemes(): Promise<string[]> {
  return invoke('get_supported_url_schemes');
}

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

export async function checkConnectionHealth(sessionId: string): Promise<string> {
  return invoke('check_connection_health', { sessionId });
}

export type ConnectionHealth = 'healthy' | 'unhealthy' | 'reconnecting';

export interface ConnectionHealthEvent {
  session_id: string;
  health: ConnectionHealth;
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
  truncated?: boolean;
  truncated_total?: number;
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

// ============================================
// ROUTINES (Functions/Procedures)
// ============================================

export type RoutineType = 'Function' | 'Procedure';

export interface Routine {
  namespace: Namespace;
  name: string;
  routine_type: RoutineType;
  arguments: string;
  return_type?: string;
  language?: string;
}

export interface RoutineList {
  routines: Routine[];
  total_count: number;
}

export async function listRoutines(
  sessionId: string,
  namespace: Namespace,
  search?: string,
  page?: number,
  pageSize?: number,
  routineType?: RoutineType
): Promise<{
  success: boolean;
  data?: RoutineList;
  error?: string;
}> {
  return invoke('list_routines', {
    sessionId,
    namespace,
    search,
    page,
    page_size: pageSize,
    routine_type: routineType,
  });
}

// ============================================
// ROUTINE DEFINITION & OPERATIONS
// ============================================

export interface RoutineDefinition {
  name: string;
  namespace: Namespace;
  routine_type: RoutineType;
  definition: string;
  language?: string;
  arguments: string;
  return_type?: string;
}

export interface RoutineOperationResult {
  success: boolean;
  executed_command: string;
  message?: string;
  execution_time_ms: number;
}

export async function getRoutineDefinition(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  routineName: string,
  routineType: RoutineType,
  routineArguments?: string
): Promise<{
  success: boolean;
  definition?: RoutineDefinition;
  error?: string;
}> {
  return invoke('get_routine_definition', {
    sessionId,
    database,
    schema,
    routineName,
    routineType,
    arguments: routineArguments,
  });
}

export async function dropRoutine(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  routineName: string,
  routineType: RoutineType,
  routineArguments?: string,
  acknowledgedDangerous?: boolean
): Promise<{
  success: boolean;
  result?: RoutineOperationResult;
  error?: string;
}> {
  return invoke('drop_routine', {
    sessionId,
    database,
    schema,
    routineName,
    routineType,
    arguments: routineArguments,
    acknowledgedDangerous,
  });
}

// ============================================
// TRIGGERS
// ============================================

export type TriggerTiming = 'Before' | 'After' | 'InsteadOf';
export type TriggerEvent = 'Insert' | 'Update' | 'Delete' | 'Truncate';

export interface Trigger {
  namespace: Namespace;
  name: string;
  table_name: string;
  timing: TriggerTiming;
  events: TriggerEvent[];
  enabled: boolean;
  function_name?: string;
}

export interface TriggerList {
  triggers: Trigger[];
  total_count: number;
}

export interface TriggerDefinition {
  name: string;
  namespace: Namespace;
  table_name: string;
  timing: TriggerTiming;
  events: TriggerEvent[];
  definition: string;
  enabled: boolean;
  function_name?: string;
}

export interface TriggerOperationResult {
  success: boolean;
  executed_command: string;
  message?: string;
  execution_time_ms: number;
}

export async function listTriggers(
  sessionId: string,
  namespace: Namespace,
  search?: string,
  page?: number,
  pageSize?: number
): Promise<{
  success: boolean;
  data?: TriggerList;
  error?: string;
}> {
  return invoke('list_triggers', {
    sessionId,
    namespace,
    search,
    page,
    page_size: pageSize,
  });
}

export async function getTriggerDefinition(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  triggerName: string
): Promise<{
  success: boolean;
  definition?: TriggerDefinition;
  error?: string;
}> {
  return invoke('get_trigger_definition', {
    sessionId,
    database,
    schema,
    triggerName,
  });
}

export async function dropTrigger(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  triggerName: string,
  tableName: string,
  acknowledgedDangerous?: boolean
): Promise<{
  success: boolean;
  result?: TriggerOperationResult;
  error?: string;
}> {
  return invoke('drop_trigger', {
    sessionId,
    database,
    schema,
    triggerName,
    tableName,
    acknowledgedDangerous,
  });
}

export async function toggleTrigger(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  triggerName: string,
  tableName: string,
  enable: boolean
): Promise<{
  success: boolean;
  result?: TriggerOperationResult;
  error?: string;
}> {
  return invoke('toggle_trigger', {
    sessionId,
    database,
    schema,
    triggerName,
    tableName,
    enable,
  });
}

// ============================================
// EVENTS (MySQL scheduled tasks)
// ============================================

export type EventStatus = 'Enabled' | 'Disabled' | 'SlavesideDisabled';

export interface DatabaseEvent {
  namespace: Namespace;
  name: string;
  event_type: string;
  interval_value?: string;
  interval_field?: string;
  status: EventStatus;
}

export interface EventList {
  events: DatabaseEvent[];
  total_count: number;
}

export interface EventDefinition {
  name: string;
  namespace: Namespace;
  definition: string;
  status: EventStatus;
}

export interface EventOperationResult {
  success: boolean;
  executed_command: string;
  message?: string;
  execution_time_ms: number;
}

export async function listEvents(
  sessionId: string,
  namespace: Namespace,
  search?: string,
  page?: number,
  pageSize?: number
): Promise<{
  success: boolean;
  data?: EventList;
  error?: string;
}> {
  return invoke('list_events', {
    sessionId,
    namespace,
    search,
    page,
    page_size: pageSize,
  });
}

export async function getEventDefinition(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  eventName: string
): Promise<{
  success: boolean;
  definition?: EventDefinition;
  error?: string;
}> {
  return invoke('get_event_definition', {
    sessionId,
    database,
    schema,
    eventName,
  });
}

export async function dropEvent(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  eventName: string,
  acknowledgedDangerous?: boolean
): Promise<{
  success: boolean;
  result?: EventOperationResult;
  error?: string;
}> {
  return invoke('drop_event', {
    sessionId,
    database,
    schema,
    eventName,
    acknowledgedDangerous,
  });
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

// ============================================
// DATABASE CREATION OPTIONS
// ============================================

export interface CollationInfo {
  name: string;
  is_default: boolean;
}

export interface CharsetInfo {
  name: string;
  description: string;
  default_collation: string;
  collations: CollationInfo[];
}

export interface CreationOptions {
  charsets: CharsetInfo[];
}

export async function getCreationOptions(sessionId: string): Promise<{
  success: boolean;
  options?: CreationOptions;
  error?: string;
}> {
  return invoke('get_creation_options', { sessionId });
}

export async function createDatabase(
  sessionId: string,
  name: string,
  options?: Record<string, unknown>,
  acknowledgedDangerous?: boolean
): Promise<{
  success: boolean;
  error?: string;
}> {
  return invoke('create_database', { sessionId, name, options, acknowledgedDangerous });
}

export async function dropDatabase(
  sessionId: string,
  name: string,
  acknowledgedDangerous?: boolean
): Promise<{
  success: boolean;
  error?: string;
}> {
  return invoke('drop_database', { sessionId, name, acknowledgedDangerous });
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
  is_virtual?: boolean;
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
  maintenance: boolean;
}

export interface DriverInfo {
  id: string;
  name: string;
  capabilities: DriverCapabilities;
}

export async function describeTable(
  sessionId: string,
  namespace: Namespace,
  table: string,
  connectionId?: string
): Promise<{
  success: boolean;
  schema?: TableSchema;
  error?: string;
}> {
  return invoke('describe_table', { sessionId, namespace, table, connectionId });
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

// ============================================
// PAGINATION TYPES AND QUERY
// ============================================

export type SortDirection = 'asc' | 'desc';

export type FilterOperator =
  | 'eq'
  | 'neq'
  | 'gt'
  | 'gte'
  | 'lt'
  | 'lte'
  | 'like'
  | 'is_null'
  | 'is_not_null';

export interface ColumnFilter {
  column: string;
  operator: FilterOperator;
  value: Value;
}

export interface TableQueryOptions {
  page?: number;
  page_size?: number;
  sort_column?: string;
  sort_direction?: SortDirection;
  filters?: ColumnFilter[];
  search?: string;
}

export interface PaginatedQueryResult {
  result: QueryResult;
  total_rows: number;
  page: number;
  page_size: number;
}

export async function queryTable(
  sessionId: string,
  namespace: Namespace,
  table: string,
  options: TableQueryOptions = {}
): Promise<{
  success: boolean;
  result?: PaginatedQueryResult;
  error?: string;
}> {
  return invoke('query_table', { sessionId, namespace, table, options });
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
// VIRTUAL RELATIONS
// ============================================

export interface VirtualRelation {
  id: string;
  source_database: string;
  source_schema?: string;
  source_table: string;
  source_column: string;
  referenced_table: string;
  referenced_column: string;
  referenced_schema?: string;
  referenced_database?: string;
  label?: string;
}

export async function listVirtualRelations(connectionId: string): Promise<{
  success: boolean;
  relations?: VirtualRelation[];
  error?: string;
}> {
  return invoke('list_virtual_relations', { connectionId });
}

export async function addVirtualRelation(
  connectionId: string,
  relation: VirtualRelation
): Promise<{ success: boolean; error?: string }> {
  return invoke('add_virtual_relation', { connectionId, relation });
}

export async function updateVirtualRelation(
  connectionId: string,
  relation: VirtualRelation
): Promise<{ success: boolean; error?: string }> {
  return invoke('update_virtual_relation', { connectionId, relation });
}

export async function deleteVirtualRelation(
  connectionId: string,
  relationId: string
): Promise<{ success: boolean; error?: string }> {
  return invoke('delete_virtual_relation', { connectionId, relationId });
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
  data: RowData,
  acknowledgedDangerous?: boolean
): Promise<MutationResponse> {
  return invoke('insert_row', { sessionId, database, schema, table, data, acknowledgedDangerous });
}

export async function updateRow(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  table: string,
  primaryKey: RowData,
  data: RowData,
  acknowledgedDangerous?: boolean
): Promise<MutationResponse> {
  return invoke('update_row', {
    sessionId,
    database,
    schema,
    table,
    primaryKey,
    data,
    acknowledgedDangerous,
  });
}

export async function deleteRow(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  table: string,
  primaryKey: RowData,
  acknowledgedDangerous?: boolean
): Promise<MutationResponse> {
  return invoke('delete_row', {
    sessionId,
    database,
    schema,
    table,
    primaryKey,
    acknowledgedDangerous,
  });
}

export async function supportsMutations(sessionId: string): Promise<boolean> {
  return invoke('supports_mutations', { sessionId });
}

// ============================================
// CSV IMPORT
// ============================================

export interface CsvPreviewResponse {
  detected_delimiter: string;
  headers: string[];
  preview_rows: string[][];
  total_lines: number;
}

export interface CsvImportConfig {
  delimiter?: string;
  has_header: boolean;
  null_string?: string;
  on_conflict?: 'skip' | 'abort';
  column_mapping?: Record<number, string>;
}

export interface ImportResponse {
  success: boolean;
  imported_rows: number;
  failed_rows: number;
  errors: string[];
  execution_time_ms: number;
}

export async function previewCsv(
  filePath: string,
  delimiter?: string,
  hasHeader?: boolean,
  previewLimit?: number
): Promise<CsvPreviewResponse> {
  return invoke('preview_csv', {
    filePath,
    delimiter,
    hasHeader,
    previewLimit,
  });
}

export async function importCsv(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  table: string,
  filePath: string,
  config: CsvImportConfig,
  acknowledgedDangerous?: boolean
): Promise<ImportResponse> {
  return invoke('import_csv', {
    sessionId,
    database,
    schema,
    table,
    filePath,
    config,
    acknowledgedDangerous,
  });
}

// ============================================
// SCHEMA EXPORT
// ============================================

export interface SchemaExportOptions {
  include_tables?: boolean;
  include_routines?: boolean;
  include_triggers?: boolean;
  include_events?: boolean;
}

export interface ExportSchemaResponse {
  success: boolean;
  table_count: number;
  routine_count: number;
  trigger_count: number;
  event_count: number;
  file_size_bytes: number;
  error?: string;
}

export async function exportSchema(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  filePath: string,
  options: SchemaExportOptions
): Promise<ExportSchemaResponse> {
  return invoke('export_schema', {
    sessionId,
    database,
    schema,
    filePath,
    options,
  });
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
  ssl_mode?: string;
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

export async function getConnectionCredentials(
  projectId: string,
  connectionId: string
): Promise<{
  success: boolean;
  password?: string;
  error?: string;
}> {
  return invoke('get_connection_credentials', { projectId, connectionId });
}

export async function deleteSavedConnection(
  projectId: string,
  connectionId: string
): Promise<VaultResponse> {
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

// ============================================
// MAINTENANCE OPERATIONS
// ============================================

export type MaintenanceOperationType =
  | 'vacuum'
  | 'analyze'
  | 'reindex'
  | 'optimize'
  | 'repair'
  | 'check'
  | 'cluster'
  | 'rebuild_indexes'
  | 'update_statistics'
  | 'compact'
  | 'validate'
  | 'integrity_check'
  | 'change_engine';

export interface MaintenanceOptions {
  full?: boolean;
  with_analyze?: boolean;
  verbose?: boolean;
  index_name?: string;
  target_engine?: string;
}

export interface MaintenanceRequest {
  operation: MaintenanceOperationType;
  options: MaintenanceOptions;
}

export interface MaintenanceOperationInfo {
  operation: MaintenanceOperationType;
  is_heavy: boolean;
  has_options: boolean;
}

export type MaintenanceMessageLevel = 'info' | 'warning' | 'error' | 'status';

export interface MaintenanceMessage {
  level: MaintenanceMessageLevel;
  text: string;
}

export interface MaintenanceResult {
  executed_command: string;
  messages: MaintenanceMessage[];
  execution_time_ms: number;
  success: boolean;
}

export async function listMaintenanceOperations(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  table: string
): Promise<{
  success: boolean;
  operations: MaintenanceOperationInfo[];
  error?: string;
}> {
  return invoke('list_maintenance_operations', { sessionId, database, schema, table });
}

export async function runMaintenance(
  sessionId: string,
  database: string,
  schema: string | null | undefined,
  table: string,
  request: MaintenanceRequest,
  acknowledgedDangerous?: boolean
): Promise<{
  success: boolean;
  result?: MaintenanceResult;
  error?: string;
}> {
  return invoke('run_maintenance', {
    sessionId,
    database,
    schema,
    table,
    request,
    acknowledgedDangerous,
  });
}

// ============================================
// SNAPSHOT COMMANDS
// ============================================

export interface SnapshotMeta {
  id: string;
  name: string;
  description?: string;
  source: string;
  source_type: string;
  connection_name?: string;
  driver?: string;
  namespace?: Namespace;
  columns: ColumnInfo[];
  row_count: number;
  created_at: string;
  file_size: number;
}

export interface SaveSnapshotRequest {
  name: string;
  description?: string;
  source: string;
  source_type: string;
  connection_name?: string;
  driver?: string;
  namespace?: Namespace;
  result: QueryResult;
}

export async function saveSnapshot(
  request: SaveSnapshotRequest
): Promise<{ success: boolean; meta?: SnapshotMeta; error?: string }> {
  return invoke('save_snapshot', { request });
}

export async function listSnapshots(): Promise<{
  success: boolean;
  snapshots: SnapshotMeta[];
  error?: string;
}> {
  return invoke('list_snapshots');
}

export async function getSnapshot(
  snapshotId: string
): Promise<{ success: boolean; result?: QueryResult; meta?: SnapshotMeta; error?: string }> {
  return invoke('get_snapshot', { snapshotId });
}

export async function deleteSnapshot(
  snapshotId: string
): Promise<{ success: boolean; error?: string }> {
  return invoke('delete_snapshot', { snapshotId });
}

export async function renameSnapshot(
  snapshotId: string,
  newName: string
): Promise<{ success: boolean; meta?: SnapshotMeta; error?: string }> {
  return invoke('rename_snapshot', { snapshotId, newName });
}
