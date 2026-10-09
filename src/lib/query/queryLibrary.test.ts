// SPDX-License-Identifier: Apache-2.0

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setActiveWorkspace } from '../stores/workspaceStore';
import type { WorkspaceInfo } from '../tauri';
import * as library from './queryLibrary';

const ipc = vi.hoisted(() => ({ wsSaveQueryLibrary: vi.fn(), wsGetQueryLibrary: vi.fn() }));
vi.mock('../tauri', () => ipc);
const storage = new Map<string, string>();
const key = (project: string) => `qoredb_query_library_v1_${project}`;

function activate(project: string) {
  setActiveWorkspace({ path: `/${project}/.qoredb`, source: 'manual' } as WorkspaceInfo, project);
}

beforeEach(() => {
  vi.useFakeTimers();
  storage.clear();
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
  });
  vi.resetAllMocks();
  ipc.wsSaveQueryLibrary.mockResolvedValue(true);
  ipc.wsGetQueryLibrary.mockResolvedValue({ version: 1, folders: [], items: [] });
  activate('a');
});
afterEach(() => {
  vi.clearAllTimers();
  vi.useRealTimers();
  vi.clearAllMocks();
  vi.unstubAllGlobals();
});

describe('workspace query library persistence', () => {
  it('binds a delayed save to its originating project', async () => {
    library.addItem({ title: 'A', query: 'SELECT 1' });
    activate('b');
    await vi.advanceTimersByTimeAsync(1000);
    expect(ipc.wsSaveQueryLibrary).toHaveBeenCalledWith(
      expect.objectContaining({ items: [expect.objectContaining({ title: 'A' })] }),
      'a'
    );
    expect(storage.has(key('b'))).toBe(false);
  });

  it('does not cancel another project’s pending save', async () => {
    library.addItem({ title: 'A', query: 'SELECT 1' });
    activate('b');
    library.addItem({ title: 'B', query: 'SELECT 2' });
    await vi.advanceTimersByTimeAsync(1000);
    expect(ipc.wsSaveQueryLibrary).toHaveBeenCalledTimes(2);
  });
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => {
    resolve = done;
  });
  return { promise, resolve };
}

it('flushes edits before the debounce fires', async () => {
  library.addItem({ title: 'A', query: 'SELECT 1' });
  await library.flushWorkspaceLibrary();
  expect(ipc.wsSaveQueryLibrary).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(1000);
  expect(ipc.wsSaveQueryLibrary).toHaveBeenCalledTimes(1);
  expect(JSON.parse(storage.get(key('a')) ?? '{}').pendingSync).toBeUndefined();
});

it('serializes edits arriving while the previous write is pending', async () => {
  const first = deferred<boolean>();
  ipc.wsSaveQueryLibrary.mockReturnValueOnce(first.promise);
  const item = library.addItem({ title: 'A', query: 'SELECT 1' });
  const flush = library.flushWorkspaceLibrary();
  library.updateItem(item.id, { query: 'SELECT 2' });
  const secondFlush = library.flushWorkspaceLibrary();
  expect(ipc.wsSaveQueryLibrary).toHaveBeenCalledTimes(1);
  first.resolve(true);
  await Promise.all([flush, secondFlush]);
  expect(ipc.wsSaveQueryLibrary).toHaveBeenCalledTimes(2);
  expect(ipc.wsSaveQueryLibrary.mock.calls[1][0].items[0].query).toBe('SELECT 2');
  expect(library.listItems()[0].query).toBe('SELECT 2');
});

it.each([
  'reject',
  'false',
])('retains unsaved edits on %s and retries before a disk reload', async failure => {
  if (failure === 'reject')
    ipc.wsSaveQueryLibrary.mockRejectedValueOnce(new Error('Disk unavailable'));
  else ipc.wsSaveQueryLibrary.mockResolvedValueOnce(false);
  library.addItem({ title: 'Unsaved', query: 'SELECT 9007199254740993' });
  const unsaved = storage.get(key('a'));
  await expect(library.flushWorkspaceLibrary()).rejects.toThrow();
  expect(storage.get(key('a'))).toBe(unsaved);
  expect(JSON.parse(unsaved ?? '{}').pendingSync).toBe(true);
  ipc.wsGetQueryLibrary.mockImplementation(async () => JSON.parse(storage.get(key('a')) ?? '{}'));
  await library.syncWorkspaceLibrary();
  expect(ipc.wsSaveQueryLibrary).toHaveBeenCalledTimes(2);
  expect(library.listItems()[0].query).toBe('SELECT 9007199254740993');
});

it('recovers pending edits from a previous session without loading older disk data', async () => {
  storage.set(
    key('a'),
    JSON.stringify({
      folders: [],
      items: [{ id: 'saved', title: 'Pending', query: 'SELECT 2', tags: [] }],
      pendingSync: true,
    })
  );
  ipc.wsSaveQueryLibrary.mockRejectedValue(new Error('Still unavailable'));
  await expect(library.syncWorkspaceLibrary()).rejects.toThrow('Still unavailable');
  expect(ipc.wsGetQueryLibrary).not.toHaveBeenCalled();
  expect(library.listItems()[0].title).toBe('Pending');
});

it('discards a disk read after switching projects', async () => {
  const read = deferred<{ version: number; folders: unknown[]; items: unknown[] }>();
  ipc.wsGetQueryLibrary.mockReturnValueOnce(read.promise);
  const sync = library.syncWorkspaceLibrary();
  await vi.advanceTimersByTimeAsync(0);
  expect(ipc.wsGetQueryLibrary).toHaveBeenCalledWith('a');
  activate('b');
  read.resolve({ version: 1, folders: [], items: [{ title: 'From A' }] });
  await sync;
  expect(storage.has(key('b'))).toBe(false);
});

it('discards a disk read when local edits arrived, even if those edits were saved', async () => {
  const read = deferred<{ version: number; folders: unknown[]; items: unknown[] }>();
  ipc.wsGetQueryLibrary.mockReturnValueOnce(read.promise);
  const sync = library.syncWorkspaceLibrary();
  await vi.advanceTimersByTimeAsync(0);
  library.addItem({ title: 'New', query: 'SELECT 2' });
  await library.flushWorkspaceLibrary();
  read.resolve({ version: 1, folders: [], items: [] });
  await sync;
  expect(library.listItems()[0].title).toBe('New');
});

it('loads and reopens the v0.1.39 library shape without losing folders, variables or exact SQL', async () => {
  // Shape verified against v0.1.39:src/lib/query/queryLibrary.ts (synthetic values).
  const legacy = {
    version: 1,
    folders: [{ id: 'folder', name: 'Archives', createdAt: 1, updatedAt: 2 }],
    items: [
      {
        id: 'query',
        title: 'Précision',
        query: 'SELECT 9007199254740993, $amount',
        folderId: 'folder',
        tags: ['finance'],
        isFavorite: true,
        driver: 'postgresql',
        database: 'fixture',
        createdAt: 1,
        updatedAt: 2,
        variables: {
          amount: { name: 'amount', type: 'number', defaultValue: '0.12345678901234567890' },
        },
      },
    ],
  };
  ipc.wsGetQueryLibrary.mockResolvedValue(legacy);
  await library.syncWorkspaceLibrary();
  expect(library.listFolders()).toEqual(legacy.folders);
  expect(library.listItems()).toEqual(legacy.items);
  library.updateItem('query', { title: 'Précision conservée' });
  await library.flushWorkspaceLibrary();
  const written = ipc.wsSaveQueryLibrary.mock.calls[0][0];
  expect(written.items[0]).toMatchObject({
    ...legacy.items[0],
    title: 'Précision conservée',
    updatedAt: expect.any(Number),
  });
  ipc.wsGetQueryLibrary.mockResolvedValue(written);
  activate('b');
  activate('a');
  await library.syncWorkspaceLibrary();
  expect(library.listItems()).toEqual(written.items);
  expect(library.listFolders()).toEqual(legacy.folders);
});

it('keeps the default profile in localStorage without filesystem IPC', async () => {
  setActiveWorkspace({ source: 'default' } as WorkspaceInfo, 'default');
  library.addItem({ title: 'Local', query: 'SELECT 1' });
  await library.flushWorkspaceLibrary();
  await library.syncWorkspaceLibrary();
  await vi.advanceTimersByTimeAsync(1000);
  expect(ipc.wsSaveQueryLibrary).not.toHaveBeenCalled();
  expect(ipc.wsGetQueryLibrary).not.toHaveBeenCalled();
  expect(storage.has('qoredb_query_library_v1')).toBe(true);
});

it('ignores an older disk response when a newer reload has completed', async () => {
  const read = deferred<{ version: number; folders: unknown[]; items: unknown[] }>();
  ipc.wsGetQueryLibrary.mockReturnValueOnce(read.promise);
  const old = library.syncWorkspaceLibrary();
  await vi.advanceTimersByTimeAsync(0);
  ipc.wsGetQueryLibrary.mockResolvedValue({
    version: 1,
    folders: [],
    items: [{ id: 'new', title: 'New' }],
  });
  await library.syncWorkspaceLibrary();
  read.resolve({ version: 1, folders: [], items: [{ id: 'old', title: 'Old' }] });
  await old;
  expect(library.listItems()[0].id).toBe('new');
});

function fullLibrary(count: number) {
  return {
    folders: [],
    items: Array.from({ length: count }, (_, i) => ({
      id: `query-${i}`,
      title: `Query ${i}`,
      query: `SELECT ${i}`,
      tags: [],
      isFavorite: true,
      createdAt: i,
      updatedAt: i,
    })),
  };
}

it('refuses a new query at capacity without silently deleting the oldest saved query', () => {
  const original = JSON.stringify(fullLibrary(300));
  storage.set(key('a'), original);
  expect(() => library.addItem({ title: 'Extra', query: 'SELECT 301' })).toThrow();
  expect(storage.get(key('a'))).toBe(original);
});

it('rejects an import that exceeds capacity without partially importing it', () => {
  const original = JSON.stringify(fullLibrary(299));
  storage.set(key('a'), original);
  expect(() => library.importLibrary({ version: 1, exportedAt: 0, ...fullLibrary(2) })).toThrow();
  expect(storage.get(key('a'))).toBe(original);
});

it('does not trim oversized legacy libraries during import', () => {
  const original = JSON.stringify(fullLibrary(301));
  storage.set(key('a'), original);
  expect(() =>
    library.importLibrary({ version: 1, exportedAt: 0, folders: [], items: [] })
  ).toThrow();
  expect(storage.get(key('a'))).toBe(original);
});

it('rejects too many imported folders without orphaning their queries', () => {
  const original = JSON.stringify({ folders: [], items: [] });
  storage.set(key('a'), original);
  const folders = Array.from({ length: 101 }, (_, i) => ({
    id: `f${i}`,
    name: `Folder ${i}`,
    createdAt: 0,
    updatedAt: 0,
  }));
  expect(() =>
    library.importLibrary({
      version: 1,
      exportedAt: 0,
      folders,
      items: [{ ...fullLibrary(1).items[0], folderId: 'f100' }],
    })
  ).toThrow();
  expect(storage.get(key('a'))).toBe(original);
});

it('saves a query and its new folder together, and leaves neither behind on failure', () => {
  const original = JSON.stringify(fullLibrary(300));
  storage.set(key('a'), original);
  expect(() =>
    library.addItem({ title: 'Extra', query: 'SELECT 301', newFolderName: 'New folder' })
  ).toThrow();
  expect(storage.get(key('a'))).toBe(original);
  storage.set(key('a'), JSON.stringify(fullLibrary(299)));
  const item = library.addItem({
    title: 'Last slot',
    query: 'SELECT 299',
    newFolderName: 'New folder',
  });
  expect(library.listFolders()).toHaveLength(1);
  expect(item.folderId).toBe(library.listFolders()[0].id);
  expect(library.listItems()).toHaveLength(300);
});

it('imports at the exact limit and merges a matching folder without duplicating it', () => {
  storage.set(key('a'), JSON.stringify(fullLibrary(299)));
  library.createFolder('Existing');
  const folder = library.listFolders()[0];
  expect(
    library.importLibrary({
      version: 1,
      exportedAt: 0,
      folders: [{ ...folder, id: 'imported' }],
      items: [{ ...fullLibrary(1).items[0], folderId: 'imported' }],
    })
  ).toEqual({ itemsImported: 1, foldersImported: 0 });
  expect(library.listFolders()).toEqual([folder]);
  expect(library.listItems()[0].folderId).toBe(folder.id);
  expect(library.listItems()).toHaveLength(300);
});
