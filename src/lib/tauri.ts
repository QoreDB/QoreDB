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
	ssh_tunnel?: SshTunnelConfig;
}

export interface SshTunnelConfig {
	host: string;
	port: number;
	username: string;
	auth: SshAuth;

	/** Security-critical host key policy */
	host_key_policy: "accept_new" | "strict" | "insecure_no_check";

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
	collection_type: "Table" | "View" | "Collection";
}

export interface QueryResult {
	columns: ColumnInfo[];
	rows: Row[];
	affected_rows?: number;
	execution_time_ms: number;
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

export async function testConnection(
	config: ConnectionConfig,
): Promise<ConnectionResponse> {
	return invoke("test_connection", { config });
}

export async function testSavedConnection(
	projectId: string,
	connectionId: string,
): Promise<ConnectionResponse> {
	return invoke("test_saved_connection", { projectId, connectionId });
}

export async function connect(
	config: ConnectionConfig,
): Promise<ConnectionResponse> {
	return invoke("connect", { config });
}

export async function connectSavedConnection(
	projectId: string,
	connectionId: string,
): Promise<ConnectionResponse> {
	return invoke("connect_saved_connection", { projectId, connectionId });
}

export async function disconnect(
	sessionId: string,
): Promise<ConnectionResponse> {
	return invoke("disconnect", { sessionId });
}

export async function listSessions(): Promise<SessionListItem[]> {
	return invoke("list_sessions");
}

// ============================================
// POLICY COMMANDS
// ============================================

export async function getSafetyPolicy(): Promise<SafetyPolicyResponse> {
	return invoke("get_safety_policy");
}

export async function setSafetyPolicy(
	policy: SafetyPolicy,
): Promise<SafetyPolicyResponse> {
	return invoke("set_safety_policy", { policy });
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
		queryId?: string;
	},
): Promise<{
	success: boolean;
	result?: QueryResult;
	error?: string;
	query_id?: string;
}> {
	return invoke("execute_query", {
		sessionId,
		query,
		acknowledgedDangerous: options?.acknowledgedDangerous,
		queryId: options?.queryId,
		timeoutMs: options?.timeoutMs,
	});
}

export async function listNamespaces(sessionId: string): Promise<{
	success: boolean;
	namespaces?: Namespace[];
	error?: string;
}> {
	return invoke("list_namespaces", { sessionId });
}

export async function listCollections(
	sessionId: string,
	namespace: Namespace,
): Promise<{
	success: boolean;
	collections?: Collection[];
	error?: string;
}> {
	return invoke("list_collections", { sessionId, namespace });
}

export async function cancelQuery(
	sessionId: string,
	queryId?: string,
): Promise<{
	success: boolean;
	error?: string;
	query_id?: string;
}> {
	return invoke("cancel_query", { sessionId, queryId });
}

// ============================================
// TABLE BROWSING
// ============================================

export interface TableSchema {
	columns: TableColumn[];
	primary_key?: string[];
	row_count_estimate?: number;
}

export interface TableColumn {
	name: string;
	data_type: string;
	nullable: boolean;
	default_value?: string;
	is_primary_key: boolean;
}

export async function describeTable(
	sessionId: string,
	namespace: Namespace,
	table: string,
): Promise<{
	success: boolean;
	schema?: TableSchema;
	error?: string;
}> {
	return invoke("describe_table", { sessionId, namespace, table });
}

export async function previewTable(
	sessionId: string,
	namespace: Namespace,
	table: string,
	limit: number = 100,
): Promise<{
	success: boolean;
	result?: QueryResult;
	error?: string;
}> {
	return invoke("preview_table", { sessionId, namespace, table, limit });
}

// ============================================
// TRANSACTIONS
// ============================================

export async function beginTransaction(sessionId: string): Promise<{
	success: boolean;
	error?: string;
}> {
	return invoke("begin_transaction", { sessionId });
}

export async function commitTransaction(sessionId: string): Promise<{
	success: boolean;
	error?: string;
}> {
	return invoke("commit_transaction", { sessionId });
}

export async function rollbackTransaction(sessionId: string): Promise<{
	success: boolean;
	error?: string;
}> {
	return invoke("rollback_transaction", { sessionId });
}

export async function supportsTransactions(
	sessionId: string,
): Promise<boolean> {
	return invoke("supports_transactions", { sessionId });
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
): Promise<MutationResponse> {
	return invoke("insert_row", { sessionId, database, schema, table, data });
}

export async function updateRow(
	sessionId: string,
	database: string,
	schema: string | null | undefined,
	table: string,
	primaryKey: RowData,
	data: RowData,
): Promise<MutationResponse> {
	return invoke("update_row", {
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
	primaryKey: RowData,
): Promise<MutationResponse> {
	return invoke("delete_row", {
		sessionId,
		database,
		schema,
		table,
		primaryKey,
	});
}

export async function supportsMutations(sessionId: string): Promise<boolean> {
	return invoke("supports_mutations", { sessionId });
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
	return invoke("export_logs");
}

// ============================================

export async function getVaultStatus(): Promise<VaultStatus> {
	return invoke("get_vault_status");
}

export async function setupMasterPassword(
	password: string,
): Promise<VaultResponse> {
	return invoke("setup_master_password", { password });
}

export async function unlockVault(password: string): Promise<VaultResponse> {
	return invoke("unlock_vault", { password });
}

export async function lockVault(): Promise<VaultResponse> {
	return invoke("lock_vault");
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
	return invoke("save_connection", { input });
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
