// SPDX-License-Identifier: Apache-2.0

import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { addItem, listItems } from '../query/queryLibrary';
import { setActiveWorkspace } from '../stores/workspaceStore';
import type { SavedConnection } from '../tauri';
import {
  buildProjectExportV1,
  importProjectExportV1,
  type ProjectExportV1,
} from './projectTransfer';

const ipc = vi.hoisted(() => ({
  listSavedConnections: vi.fn(),
  saveConnection: vi.fn(),
  wsSaveQueryLibrary: vi.fn(),
  wsGetQueryLibrary: vi.fn(),
}));
vi.mock('../tauri', () => ipc);
const storage = new Map<string, string>();
const connection = {
  id: 'original',
  name: 'Synthetic',
  driver: 'postgres',
  host: 'localhost',
  port: 5432,
  username: 'user',
  environment: 'development',
  read_only: true,
  ssl: false,
} as SavedConnection;
function payload(): ProjectExportV1 & {
  queryLibrary: NonNullable<ProjectExportV1['queryLibrary']>;
} {
  return {
    type: 'qoredb_project',
    version: 1,
    exportedAt: 0,
    projectId: 'source',
    credentialsIncluded: false,
    connections: [connection],
    queryLibrary: {
      version: 1,
      exportedAt: 0,
      folders: [],
      items: [
        {
          id: 'query',
          title: 'Imported',
          query: 'SELECT 42',
          tags: [],
          isFavorite: false,
          createdAt: 0,
          updatedAt: 0,
        },
      ],
    },
  };
}
beforeEach(() => {
  storage.clear();
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
  });
  vi.resetAllMocks();
  ipc.listSavedConnections.mockResolvedValue([]);
  ipc.saveConnection.mockResolvedValue({ success: true });
  setActiveWorkspace(null, 'a');
});
afterEach(() => vi.unstubAllGlobals());

it('round-trips local and passwordless connection formats exported by the app', async () => {
  for (const config of [
    { driver: 'sqlite', host: '/synthetic/fixture.db', port: 0 },
    { driver: 'duckdb', host: '/synthetic/fixture.duckdb', port: 0 },
    { driver: 'redis', host: 'localhost', port: 6379 },
    { driver: 'clickhouse', host: 'localhost', port: 8123 },
    {
      driver: 'sqlserver',
      host: 'localhost',
      port: 1433,
      mssql_auth: 'windows_integrated' as const,
    },
  ]) {
    const data = payload();
    data.connections = [{ ...connection, ...config, username: '' }];
    const result = await importProjectExportV1(data, { projectId: 'a' });
    expect(result.connectionsImported, config.driver).toBe(1);
    expect(ipc.saveConnection).toHaveBeenLastCalledWith(
      expect.objectContaining({ ...config, username: '' })
    );
  }
});

it('does not retry a licence-rejected import without its masking rules', async () => {
  const data = payload();
  data.connections[0] = {
    ...connection,
    masking: {
      rules: [{ table: 'items', column: 'email', mode: 'hidden' }],
      mask_detected_columns: false,
    },
  };
  ipc.saveConnection.mockResolvedValue({
    success: false,
    error: 'Column masking requires a QoreDB Pro license',
  });
  const result = await importProjectExportV1(data, { projectId: 'a' });
  expect(result.connectionsImported).toBe(0);
  expect(result.connectionsSkipped).toBe(1);
  expect(ipc.saveConnection).toHaveBeenCalledTimes(1);
  expect(ipc.saveConnection).toHaveBeenCalledWith(
    expect.objectContaining({ masking: data.connections[0].masking })
  );
});

it('preserves exported safety and transport metadata without importing secrets', async () => {
  const data = payload();
  const metadata = {
    ssl_mode: 'verify-full',
    ssl_ca_cert: '/synthetic/ca.pem',
    options: { application_name: 'qualification' },
    mssql_auth: 'sql_password' as const,
    clickhouse_cluster: 'synthetic',
    search_auth_mode: 'basic' as const,
    masking: {
      rules: [{ table: 'items', column: 'email', mode: 'hidden' as const }],
      mask_detected_columns: true,
    },
    proxy: {
      proxy_type: 'socks5',
      host: 'localhost',
      port: 1080,
      username: 'synthetic',
      connect_timeout_secs: 10,
    },
  };
  data.connections = [
    {
      ...connection,
      ...metadata,
      password: 'must-not-import',
      proxy: { ...metadata.proxy, password: 'must-not-import' },
    } as SavedConnection,
  ];
  await importProjectExportV1(data, { projectId: 'a' });
  expect(ipc.saveConnection).toHaveBeenCalledWith(
    expect.objectContaining({
      ...metadata,
      password: '',
      proxy: expect.objectContaining({ ...metadata.proxy, password: undefined }),
    })
  );
});

it('rejects an export if its originating workspace changed during connection loading', async () => {
  let resolve!: (connections: SavedConnection[]) => void;
  ipc.listSavedConnections.mockReturnValueOnce(
    new Promise(done => {
      resolve = done;
    })
  );
  addItem({ title: 'A', query: 'SELECT 1' });
  const pending = buildProjectExportV1({
    projectId: 'a',
    includeQueryLibrary: true,
    redactQueries: false,
  });
  setActiveWorkspace(null, 'b');
  addItem({ title: 'B', query: 'SELECT 2' });
  resolve([connection]);
  await expect(pending).rejects.toThrow();
});

it('rejects a malformed library before saving any connections', async () => {
  const data = payload();
  data.queryLibrary.items[0].query = '';
  await expect(importProjectExportV1(data, { projectId: 'a' })).rejects.toThrow();
  expect(ipc.saveConnection).not.toHaveBeenCalled();
  expect(listItems()).toEqual([]);
});

it('does not import a library into a workspace selected while a connection is being saved', async () => {
  let resolve!: (result: { success: boolean }) => void;
  ipc.saveConnection.mockReturnValueOnce(
    new Promise(done => {
      resolve = done;
    })
  );
  const pending = importProjectExportV1(payload(), { projectId: 'a' });
  await vi.waitFor(() => expect(ipc.saveConnection).toHaveBeenCalled());
  setActiveWorkspace(null, 'b');
  resolve({ success: true });
  await expect(pending).rejects.toThrow();
  expect(listItems()).toEqual([]);
  expect(ipc.saveConnection.mock.calls[0][0].project_id).toBe('a');
});

it('does not assume no name conflicts when listing existing connections fails', async () => {
  ipc.listSavedConnections.mockRejectedValueOnce(new Error('Synthetic read error'));
  await expect(importProjectExportV1(payload(), { projectId: 'a' })).rejects.toThrow();
  expect(ipc.saveConnection).not.toHaveBeenCalled();
});

it('rejects capacity overflow before saving any connections', async () => {
  const data = payload();
  storage.set(
    'qoredb_query_library_v1_a',
    JSON.stringify({
      folders: [],
      items: Array.from({ length: 300 }, (_, i) => ({
        ...data.queryLibrary.items[0],
        id: String(i),
      })),
    })
  );
  await expect(importProjectExportV1(data, { projectId: 'a' })).rejects.toThrow();
  expect(ipc.saveConnection).not.toHaveBeenCalled();
  expect(listItems()).toHaveLength(300);
});

it('does not resume an import after returning to its original workspace', async () => {
  let resolve!: (connections: SavedConnection[]) => void;
  ipc.listSavedConnections.mockReturnValueOnce(
    new Promise(done => {
      resolve = done;
    })
  );
  const pending = importProjectExportV1(payload(), { projectId: 'a' });
  setActiveWorkspace(null, 'b');
  setActiveWorkspace(null, 'a');
  resolve([]);
  await expect(pending).rejects.toThrow();
  expect(ipc.saveConnection).not.toHaveBeenCalled();
  expect(listItems()).toEqual([]);
});

it('preserves concurrent local edits when committing an import', async () => {
  let resolve!: (result: { success: boolean }) => void;
  ipc.saveConnection.mockReturnValueOnce(
    new Promise(done => {
      resolve = done;
    })
  );
  const pending = importProjectExportV1(payload(), { projectId: 'a' });
  await vi.waitFor(() => expect(ipc.saveConnection).toHaveBeenCalled());
  addItem({ title: 'Concurrent', query: 'SELECT 99' });
  resolve({ success: true });
  expect((await pending).libraryImported?.itemsImported).toBe(1);
  expect(
    listItems()
      .map(i => i.title)
      .sort()
  ).toEqual(['Concurrent', 'Imported']);
});

it('imports a valid project with collision renaming and preserved safety flags', async () => {
  ipc.listSavedConnections.mockResolvedValue([connection]);
  const result = await importProjectExportV1(payload(), { projectId: 'a' });
  expect(result).toMatchObject({
    connectionsImported: 1,
    connectionsSkipped: 0,
    libraryImported: { itemsImported: 1 },
  });
  expect(ipc.saveConnection.mock.calls[0][0]).toMatchObject({
    name: 'Synthetic (imported)',
    password: '',
    project_id: 'a',
    read_only: true,
    environment: 'development',
  });
  expect(listItems()[0].query).toBe('SELECT 42');
});

it('exports a stable library snapshot and honors omission', async () => {
  addItem({ title: 'Original', query: 'SELECT 9007199254740993' });
  const result = await buildProjectExportV1({
    projectId: 'a',
    includeQueryLibrary: true,
    redactQueries: false,
  });
  expect(result.queryLibrary?.items[0].query).toBe('SELECT 9007199254740993');
  const omitted = await buildProjectExportV1({
    projectId: 'a',
    includeQueryLibrary: false,
    redactQueries: false,
  });
  expect(omitted.queryLibrary).toBeUndefined();
});
