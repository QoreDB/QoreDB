// SPDX-License-Identifier: Apache-2.0

import i18n from '../../i18n';
import type { NotebookVariable } from '../notebook/notebookTypes';
import { redactQuery } from '../redaction';
import { getWorkspaceState } from '../stores/workspaceStore';
import { wsGetQueryLibrary, wsSaveQueryLibrary } from '../tauri';

/**
 * A parameter placeholder (`{{name}}` / `$name`) defined on a saved query.
 * Reuses the notebook variable shape so substitution and the typed inputs are
 * shared between notebooks and the query library.
 */
export type QueryVariable = NotebookVariable;

export interface QueryFolder {
  id: string;
  name: string;
  createdAt: number;
  updatedAt: number;
}

export interface QueryLibraryItem {
  id: string;
  title: string;
  query: string;
  folderId?: string | null;
  tags: string[];
  isFavorite: boolean;
  driver?: string;
  database?: string;
  variables?: Record<string, QueryVariable>;
  createdAt: number;
  updatedAt: number;
}

export interface QueryLibraryExportV1 {
  version: 1;
  exportedAt: number;
  folders: QueryFolder[];
  items: QueryLibraryItem[];
}

const STORAGE_KEY_PREFIX = 'qoredb_query_library_v1';
const MAX_ITEMS = 300;
const MAX_FOLDERS = 100;

interface QueryLibraryState {
  folders: QueryFolder[];
  items: QueryLibraryItem[];
}

function now(): number {
  return Date.now();
}

function generateId(prefix: string): string {
  const rand = Math.random().toString(36).slice(2, 9);
  return `${prefix}_${Date.now()}_${rand}`;
}

function normalizeTag(tag: string): string {
  return tag.trim().replace(/\s+/g, ' ').toLowerCase();
}

export function parseTags(raw: string): string[] {
  if (!raw.trim()) return [];
  const parts = raw
    .split(',')
    .map(part => normalizeTag(part))
    .filter(Boolean);
  const unique = Array.from(new Set(parts));
  return unique.slice(0, 12);
}

function getStorageKey(projectId = getWorkspaceState().projectId): string {
  return projectId === 'default' ? STORAGE_KEY_PREFIX : `${STORAGE_KEY_PREFIX}_${projectId}`;
}

function readState(): QueryLibraryState {
  try {
    const raw = localStorage.getItem(getStorageKey());
    if (!raw) return { folders: [], items: [] };
    const parsed = JSON.parse(raw) as Partial<QueryLibraryState>;
    return {
      folders: Array.isArray(parsed.folders) ? parsed.folders : [],
      items: Array.isArray(parsed.items) ? parsed.items : [],
    };
  } catch {
    return { folders: [], items: [] };
  }
}

const listeners = new Set<() => void>();

export function subscribeQueryLibrary(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function notifyLibraryChanged() {
  for (const listener of listeners) listener();
}

const syncTimers = new Map<string, ReturnType<typeof setTimeout>>();
const saves = new Map<string, Promise<void>>();
const loads = new Map<string, symbol>();
const SYNC_DEBOUNCE_MS = 1000;

function writeState(next: QueryLibraryState): void {
  const { projectId, activeWorkspace, isLoading } = getWorkspaceState();
  if (isLoading) throw new Error(i18n.t('common.loading'));
  const pendingSync = !!activeWorkspace && activeWorkspace.source !== 'default';
  localStorage.setItem(getStorageKey(projectId), JSON.stringify({ ...next, pendingSync }));
  notifyLibraryChanged();

  if (pendingSync) {
    clearTimeout(syncTimers.get(projectId));
    syncTimers.set(
      projectId,
      setTimeout(() => {
        syncTimers.delete(projectId);
        void flushWorkspaceLibrary(projectId).catch(() => {
          // Keep the pending flag across restarts; loading from disk must not erase unsaved edits.
          console.warn('Query library sync failed; local changes retained.');
        });
      }, SYNC_DEBOUNCE_MS)
    );
  }
}

/** Finish all local edits for this project before changing the backend workspace. */
export async function flushWorkspaceLibrary(
  projectId = getWorkspaceState().projectId
): Promise<void> {
  clearTimeout(syncTimers.get(projectId));
  syncTimers.delete(projectId);
  const previous = saves.get(projectId);
  if (previous) {
    await previous;
    return flushWorkspaceLibrary(projectId);
  }

  const key = getStorageKey(projectId);
  const save = async () => {
    while (true) {
      const raw = localStorage.getItem(key);
      if (!raw) return;
      const state = JSON.parse(raw) as QueryLibraryState & { pendingSync?: boolean };
      if (!state.pendingSync) return;
      const saved = await wsSaveQueryLibrary(
        {
          version: 1,
          folders: state.folders,
          items: state.items,
        },
        projectId
      );
      if (!saved) throw new Error('Query library was not saved');
      if (localStorage.getItem(key) === raw) {
        localStorage.setItem(key, JSON.stringify({ folders: state.folders, items: state.items }));
        return;
      }
      // A new edit arrived during IO; serialize its write after the older snapshot.
    }
  };
  const pending = save();
  saves.set(projectId, pending);
  try {
    await pending;
  } finally {
    saves.delete(projectId);
  }
}

/** A late disk read must not replace another project's library or newer local edits. */
export async function syncWorkspaceLibrary(): Promise<void> {
  const { projectId, activeWorkspace } = getWorkspaceState();
  if (!activeWorkspace || activeWorkspace.source === 'default') return;
  const load = Symbol();
  loads.set(projectId, load);
  try {
    await flushWorkspaceLibrary(projectId);
    const key = getStorageKey(projectId);
    const before = localStorage.getItem(key);
    const data = await wsGetQueryLibrary(projectId);
    if (
      data &&
      loads.get(projectId) === load &&
      getWorkspaceState().projectId === projectId &&
      localStorage.getItem(key) === before
    ) {
      localStorage.setItem(key, JSON.stringify({ folders: data.folders, items: data.items }));
      notifyLibraryChanged();
    }
  } finally {
    if (loads.get(projectId) === load) loads.delete(projectId);
  }
}

export function listFolders(): QueryFolder[] {
  return readState()
    .folders.slice()
    .sort((a, b) => a.name.localeCompare(b.name));
}

function prepareFolder(state: QueryLibraryState, name: string): QueryFolder {
  const trimmed = name.trim();
  if (!trimmed) {
    throw new Error('Folder name is required');
  }

  const exists = state.folders.some(f => f.name.toLowerCase() === trimmed.toLowerCase());
  if (exists) {
    return state.folders.find(f => f.name.toLowerCase() === trimmed.toLowerCase()) as QueryFolder;
  }

  if (state.folders.length >= MAX_FOLDERS) {
    throw new Error(i18n.t('library.folderLimit', { count: MAX_FOLDERS }));
  }

  const folder: QueryFolder = {
    id: generateId('folder'),
    name: trimmed,
    createdAt: now(),
    updatedAt: now(),
  };

  state.folders.push(folder);

  return folder;
}

export function createFolder(name: string): QueryFolder {
  const state = readState();
  const count = state.folders.length;
  const folder = prepareFolder(state, name);
  if (state.folders.length !== count) writeState(state);
  return folder;
}

export function renameFolder(folderId: string, name: string): QueryFolder {
  const trimmed = name.trim();
  if (!trimmed) {
    throw new Error('Folder name is required');
  }

  const state = readState();
  const folder = state.folders.find(f => f.id === folderId);
  if (!folder) {
    throw new Error('Folder not found');
  }

  const conflict = state.folders.some(
    f => f.id !== folderId && f.name.toLowerCase() === trimmed.toLowerCase()
  );
  if (conflict) {
    throw new Error('Folder name already exists');
  }

  const updated: QueryFolder = { ...folder, name: trimmed, updatedAt: now() };
  writeState({
    ...state,
    folders: state.folders.map(f => (f.id === folderId ? updated : f)),
  });
  return updated;
}

export function deleteFolder(folderId: string): void {
  const state = readState();
  const folderExists = state.folders.some(f => f.id === folderId);
  if (!folderExists) return;

  writeState({
    folders: state.folders.filter(f => f.id !== folderId),
    items: state.items.map(item =>
      item.folderId === folderId ? { ...item, folderId: null, updatedAt: now() } : item
    ),
  });
}

export function listItems(options?: {
  folderId?: string | null;
  search?: string;
  tag?: string;
  favoritesOnly?: boolean;
}): QueryLibraryItem[] {
  const state = readState();
  const search = options?.search?.trim().toLowerCase();
  const tag = options?.tag ? normalizeTag(options.tag) : undefined;

  return state.items
    .filter(item => {
      if (options?.favoritesOnly && !item.isFavorite) return false;
      if (options?.folderId !== undefined) {
        const folderId = options.folderId ?? null;
        if ((item.folderId ?? null) !== folderId) return false;
      }
      if (tag && !item.tags.includes(tag)) return false;
      if (search) {
        const haystack = `${item.title}\n${item.query}`.toLowerCase();
        if (!haystack.includes(search)) return false;
      }
      return true;
    })
    .slice()
    .sort((a, b) => b.updatedAt - a.updatedAt);
}

/**
 * Strips runtime-only values from variable definitions so the library only
 * persists the typed schema (name, type, default, options, description).
 */
function sanitizeVariables(
  variables?: Record<string, QueryVariable>
): Record<string, QueryVariable> | undefined {
  if (!variables) return undefined;
  const entries = Object.entries(variables);
  if (entries.length === 0) return undefined;
  return Object.fromEntries(entries.map(([name, v]) => [name, { ...v, currentValue: undefined }]));
}

export function addItem(input: {
  title: string;
  query: string;
  folderId?: string | null;
  newFolderName?: string;
  tags?: string[];
  isFavorite?: boolean;
  driver?: string;
  database?: string;
  variables?: Record<string, QueryVariable>;
}): QueryLibraryItem {
  const title = input.title.trim();
  const query = input.query;

  if (!title) throw new Error('Title is required');
  if (!query.trim()) throw new Error('Query is required');

  const state = readState();

  const item: QueryLibraryItem = {
    id: generateId('ql'),
    title,
    query,
    folderId:
      input.newFolderName !== undefined
        ? prepareFolder(state, input.newFolderName).id
        : (input.folderId ?? null),
    tags: Array.from(new Set((input.tags ?? []).map(normalizeTag).filter(Boolean))).slice(0, 12),
    isFavorite: input.isFavorite ?? false,
    driver: input.driver,
    database: input.database,
    variables: sanitizeVariables(input.variables),
    createdAt: now(),
    updatedAt: now(),
  };

  if (state.items.length >= MAX_ITEMS) {
    throw new Error(i18n.t('library.itemLimit', { count: MAX_ITEMS }));
  }
  const nextItems = [item, ...state.items];

  writeState({ ...state, items: nextItems });
  return item;
}

export function updateItem(
  id: string,
  patch: Partial<
    Pick<QueryLibraryItem, 'title' | 'query' | 'folderId' | 'tags' | 'isFavorite' | 'variables'>
  >
): QueryLibraryItem {
  const state = readState();
  const existing = state.items.find(i => i.id === id);
  if (!existing) throw new Error('Item not found');

  const next: QueryLibraryItem = {
    ...existing,
    title: patch.title !== undefined ? patch.title.trim() : existing.title,
    query: patch.query !== undefined ? patch.query : existing.query,
    folderId: patch.folderId !== undefined ? (patch.folderId ?? null) : existing.folderId,
    tags:
      patch.tags !== undefined
        ? Array.from(new Set(patch.tags.map(normalizeTag).filter(Boolean))).slice(0, 12)
        : existing.tags,
    isFavorite: patch.isFavorite !== undefined ? patch.isFavorite : existing.isFavorite,
    variables:
      patch.variables !== undefined ? sanitizeVariables(patch.variables) : existing.variables,
    updatedAt: now(),
  };

  if (!next.title) throw new Error('Title is required');
  if (!next.query.trim()) throw new Error('Query is required');

  writeState({
    ...state,
    items: state.items.map(i => (i.id === id ? next : i)),
  });

  return next;
}

export function deleteItem(id: string): void {
  const state = readState();
  writeState({ ...state, items: state.items.filter(i => i.id !== id) });
}

export function exportLibrary(options?: { redact?: boolean }): QueryLibraryExportV1 {
  const state = readState();
  const redact = options?.redact ?? false;

  return {
    version: 1,
    exportedAt: now(),
    folders: state.folders,
    items: state.items.map(item => (redact ? { ...item, query: redactQuery(item.query) } : item)),
  };
}

export function importLibrary(payload: QueryLibraryExportV1): {
  foldersImported: number;
  itemsImported: number;
} {
  if (payload.version !== 1) {
    throw new Error('Unsupported library export version');
  }

  const state = readState();
  const folderNameToId = new Map<string, string>();
  for (const folder of state.folders) {
    folderNameToId.set(folder.name.toLowerCase(), folder.id);
  }

  const importedFolders: QueryFolder[] = [];
  for (const folder of payload.folders ?? []) {
    const name = (folder?.name ?? '').trim();
    if (!name) continue;
    const existingId = folderNameToId.get(name.toLowerCase());
    if (existingId) continue;

    const created: QueryFolder = {
      id: generateId('folder'),
      name,
      createdAt: now(),
      updatedAt: now(),
    };
    folderNameToId.set(name.toLowerCase(), created.id);
    importedFolders.push(created);
  }

  const folderIdMap = new Map<string, string>();
  for (const folder of payload.folders ?? []) {
    const name = (folder?.name ?? '').trim();
    if (!name) continue;
    const mapped = folderNameToId.get(name.toLowerCase());
    if (mapped) folderIdMap.set(folder.id, mapped);
  }

  const importedItems: QueryLibraryItem[] = [];
  for (const item of payload.items ?? []) {
    const title = (item?.title ?? '').trim();
    const query = item?.query ?? '';
    if (!title || !query.trim()) continue;
    const folderId =
      item.folderId && folderIdMap.has(item.folderId) ? folderIdMap.get(item.folderId) : null;
    importedItems.push({
      id: generateId('ql'),
      title,
      query,
      folderId,
      tags: Array.from(new Set((item.tags ?? []).map(normalizeTag).filter(Boolean))).slice(0, 12),
      isFavorite: !!item.isFavorite,
      driver: item.driver,
      database: item.database,
      variables: sanitizeVariables(item.variables),
      createdAt: now(),
      updatedAt: now(),
    });
  }

  if (state.items.length + importedItems.length > MAX_ITEMS) {
    throw new Error(i18n.t('library.itemLimit', { count: MAX_ITEMS }));
  }
  if (state.folders.length + importedFolders.length > MAX_FOLDERS) {
    throw new Error(i18n.t('library.folderLimit', { count: MAX_FOLDERS }));
  }
  writeState({
    folders: [...state.folders, ...importedFolders],
    items: [...importedItems, ...state.items],
  });

  return { foldersImported: importedFolders.length, itemsImported: importedItems.length };
}
