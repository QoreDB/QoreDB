// SPDX-License-Identifier: Apache-2.0

import i18n from '../../i18n';
import { Driver } from '../connection/drivers';
import type { QueryLibraryExportV1 } from '../query/queryLibrary';
import { exportLibrary, importLibrary, validateLibraryImport } from '../query/queryLibrary';
import { captureWorkspaceScope } from '../stores/workspaceStore';
import type { Environment, SavedConnection } from '../tauri';
import { listSavedConnections, saveConnection } from '../tauri';

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

function isSupportedDriver(driver: unknown): driver is Driver {
  return typeof driver === 'string' && Object.values(Driver).includes(driver as Driver);
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
  const isCurrent = captureWorkspaceScope(input.projectId);
  if (!isCurrent()) throw new Error(i18n.t('common.contextChanged'));
  const queryLibrary = input.includeQueryLibrary
    ? exportLibrary({ redact: input.redactQueries })
    : undefined;
  const connections = await listSavedConnections(input.projectId);
  if (!isCurrent()) throw new Error(i18n.t('common.contextChanged'));

  return {
    type: 'qoredb_project',
    version: 1,
    exportedAt: Date.now(),
    projectId: input.projectId,
    credentialsIncluded: false,
    connections,
    queryLibrary,
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
  const isCurrent = captureWorkspaceScope(input.projectId);
  const assertCurrent = () => {
    if (!isCurrent()) throw new Error(i18n.t('common.contextChanged'));
  };
  assertCurrent();
  if (payload.queryLibrary !== undefined) validateLibraryImport(payload.queryLibrary);
  const existing = await listSavedConnections(input.projectId);
  assertCurrent();
  const existingNames = new Set(existing.map(c => c.name));

  let connectionsImported = 0;
  let connectionsSkipped = 0;

  for (const raw of payload.connections.slice(0, maxConnections)) {
    assertCurrent();
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

    if (
      !name ||
      !isSupportedDriver(driver) ||
      !host ||
      port === undefined ||
      !Number.isInteger(port) ||
      port < 0 ||
      port > 65535 ||
      username === undefined
    ) {
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

    // Keep exported transport/privacy fields. Rust validates their shape and licence;
    // dropping them here could silently weaken TLS or remove masking on import.
    const metadata = raw as unknown as SavedConnection;
    const ssh_tunnel =
      metadata.ssh_tunnel == null
        ? undefined
        : {
            ...metadata.ssh_tunnel,
            password: undefined,
            key_passphrase: undefined,
          };
    const proxy =
      metadata.proxy == null
        ? undefined
        : {
            ...metadata.proxy,
            password: undefined,
          };

    const result = await saveConnection({
      ...metadata,
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
      proxy,
    });
    assertCurrent();

    if (result.success) {
      connectionsImported += 1;
    } else {
      connectionsSkipped += 1;
    }
  }

  const queryLibrary = payload.queryLibrary;
  assertCurrent();
  const libraryImported = queryLibrary !== undefined ? importLibrary(queryLibrary) : undefined;

  return { connectionsImported, connectionsSkipped, libraryImported };
}
