import type { Environment, SavedConnection } from './tauri';
import type { QueryLibraryExportV1 } from './queryLibrary';

import { listSavedConnections, saveConnection } from './tauri';
import { exportLibrary, importLibrary } from './queryLibrary';

export interface ProjectExportV1 {
  type: 'qoredb_project';
  version: 1;
  exportedAt: number;
  projectId: string;
  credentialsIncluded: false;
  connections: SavedConnection[];
  queryLibrary?: QueryLibraryExportV1;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isEnvironment(value: unknown): value is Environment {
  return value === 'development' || value === 'staging' || value === 'production';
}

function asString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

function asBoolean(value: unknown): boolean | undefined {
  return typeof value === 'boolean' ? value : undefined;
}

function asNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

function isSupportedDriver(driver: unknown): driver is 'postgres' | 'mysql' | 'mongodb' {
  return driver === 'postgres' || driver === 'mysql' || driver === 'mongodb';
}

function makeImportedName(baseName: string, existingNames: Set<string>): string {
  const candidate = `${baseName} (imported)`;
  if (!existingNames.has(candidate)) return candidate;

  let index = 2;
  while (index < 1000) {
    const candidate = `${baseName} (imported ${index})`;
    if (!existingNames.has(candidate)) return candidate;
    index += 1;
  }
  return `${baseName} (imported ${Date.now()})`;
}

function generateConnectionId(): string {
  const uuid =
    typeof crypto !== 'undefined' && 'randomUUID' in crypto
      ? crypto.randomUUID()
      : `${Date.now()}_${Math.random().toString(16).slice(2)}`;
  return `conn_${uuid.replace(/-/g, '')}`;
}

export async function buildProjectExportV1(input: {
  projectId: string;
  includeQueryLibrary: boolean;
  redactQueries: boolean;
}): Promise<ProjectExportV1> {
  const connections = await listSavedConnections(input.projectId);

  return {
    type: 'qoredb_project',
    version: 1,
    exportedAt: Date.now(),
    projectId: input.projectId,
    credentialsIncluded: false,
    connections,
    queryLibrary: input.includeQueryLibrary
      ? exportLibrary({ redact: input.redactQueries })
      : undefined,
  };
}

export function isProjectExportV1(value: unknown): value is ProjectExportV1 {
  if (!isRecord(value)) return false;
  if (value.type !== 'qoredb_project') return false;
  if (value.version !== 1) return false;
  if (value.credentialsIncluded !== false) return false;
  if (typeof value.projectId !== 'string') return false;
  if (!Array.isArray(value.connections)) return false;
  return true;
}

export async function importProjectExportV1(
  payload: ProjectExportV1,
  input: { projectId: string; maxConnections?: number }
): Promise<{
  connectionsImported: number;
  connectionsSkipped: number;
  libraryImported?: { foldersImported: number; itemsImported: number };
}> {
  const maxConnections = input.maxConnections ?? 100;

  let existingNames = new Set<string>();
  try {
    const existing = await listSavedConnections(input.projectId);
    existingNames = new Set(existing.map(c => c.name));
  } catch {
    existingNames = new Set();
  }

  let connectionsImported = 0;
  let connectionsSkipped = 0;

  for (const raw of payload.connections.slice(0, maxConnections)) {
    if (!isRecord(raw)) {
      connectionsSkipped += 1;
      continue;
    }

    const name = asString(raw.name)?.trim();
    const driver = raw.driver;
    const host = asString(raw.host)?.trim();
    const port = asNumber(raw.port);
    const username = asString(raw.username)?.trim();
    const environment = raw.environment;
    const readOnly = asBoolean(raw.read_only);
    const ssl = asBoolean(raw.ssl);

    if (!name || !isSupportedDriver(driver) || !host || !port || !username) {
      connectionsSkipped += 1;
      continue;
    }
    if (!isEnvironment(environment)) {
      connectionsSkipped += 1;
      continue;
    }
    if (readOnly === undefined || ssl === undefined) {
      connectionsSkipped += 1;
      continue;
    }

    const id = generateConnectionId();
    const resolvedName = existingNames.has(name) ? makeImportedName(name, existingNames) : name;
    existingNames.add(resolvedName);

    const database = asString(raw.database)?.trim();

    const pool_max_connections = asNumber(raw.pool_max_connections);
    const pool_min_connections = asNumber(raw.pool_min_connections);
    const pool_acquire_timeout_secs = asNumber(raw.pool_acquire_timeout_secs);

    const sshTunnelRaw = isRecord(raw.ssh_tunnel) ? raw.ssh_tunnel : undefined;
    const ssh_tunnel = sshTunnelRaw
      ? (() => {
          const host = asString(sshTunnelRaw.host)?.trim();
          const port = asNumber(sshTunnelRaw.port);
          const username = asString(sshTunnelRaw.username)?.trim();
          const auth_type = asString(sshTunnelRaw.auth_type)?.trim();
          const key_path = asString(sshTunnelRaw.key_path);
          const host_key_policy = asString(sshTunnelRaw.host_key_policy)?.trim();
          const proxy_jump = asString(sshTunnelRaw.proxy_jump)?.trim();
          const connect_timeout_secs = asNumber(sshTunnelRaw.connect_timeout_secs) ?? 10;
          const keepalive_interval_secs = asNumber(sshTunnelRaw.keepalive_interval_secs) ?? 30;
          const keepalive_count_max = asNumber(sshTunnelRaw.keepalive_count_max) ?? 3;

          const allowedHostKeyPolicies = new Set(['accept_new', 'strict', 'insecure_no_check']);
          const resolvedHostKeyPolicy =
            host_key_policy && allowedHostKeyPolicies.has(host_key_policy)
              ? host_key_policy
              : 'accept_new';

          if (!host || !port || !username || !auth_type) return undefined;

          return {
            host,
            port,
            username,
            auth_type,
            key_path: key_path || undefined,
            host_key_policy: resolvedHostKeyPolicy,
            proxy_jump: proxy_jump || undefined,
            connect_timeout_secs,
            keepalive_interval_secs,
            keepalive_count_max,
          };
        })()
      : undefined;

    const result = await saveConnection({
      id,
      name: resolvedName,
      driver,
      environment,
      read_only: readOnly,
      host,
      port,
      username,
      password: '',
      database: database || undefined,
      ssl,
      pool_max_connections: pool_max_connections ?? undefined,
      pool_min_connections: pool_min_connections ?? undefined,
      pool_acquire_timeout_secs: pool_acquire_timeout_secs ?? undefined,
      project_id: input.projectId,
      ssh_tunnel,
    });

    if (result.success) {
      connectionsImported += 1;
    } else {
      connectionsSkipped += 1;
    }
  }

  const queryLibrary = payload.queryLibrary;
  const libraryImported = queryLibrary ? importLibrary(queryLibrary) : undefined;

  return { connectionsImported, connectionsSkipped, libraryImported };
}
