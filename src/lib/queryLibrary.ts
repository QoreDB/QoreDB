import { redactQuery } from './redaction';

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
  createdAt: number;
  updatedAt: number;
}

export interface QueryLibraryExportV1 {
  version: 1;
  exportedAt: number;
  folders: QueryFolder[];
  items: QueryLibraryItem[];
}

const STORAGE_KEY = 'qoredb_query_library_v1';
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

function readState(): QueryLibraryState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
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

function writeState(next: QueryLibraryState): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
}

export function listFolders(): QueryFolder[] {
  return readState().folders.slice().sort((a, b) => a.name.localeCompare(b.name));
}

export function createFolder(name: string): QueryFolder {
  const trimmed = name.trim();
  if (!trimmed) {
    throw new Error('Folder name is required');
  }

  const state = readState();
  const exists = state.folders.some(f => f.name.toLowerCase() === trimmed.toLowerCase());
  if (exists) {
    return state.folders.find(f => f.name.toLowerCase() === trimmed.toLowerCase()) as QueryFolder;
  }

  if (state.folders.length >= MAX_FOLDERS) {
    throw new Error('Too many folders');
  }

  const folder: QueryFolder = {
    id: generateId('folder'),
    name: trimmed,
    createdAt: now(),
    updatedAt: now(),
  };

  writeState({
    ...state,
    folders: [...state.folders, folder],
  });

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

export function addItem(input: {
  title: string;
  query: string;
  folderId?: string | null;
  tags?: string[];
  isFavorite?: boolean;
  driver?: string;
  database?: string;
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
    folderId: input.folderId ?? null,
    tags: Array.from(new Set((input.tags ?? []).map(normalizeTag).filter(Boolean))).slice(0, 12),
    isFavorite: input.isFavorite ?? false,
    driver: input.driver,
    database: input.database,
    createdAt: now(),
    updatedAt: now(),
  };

  const nextItems = [item, ...state.items];
  if (nextItems.length > MAX_ITEMS) {
    nextItems.splice(MAX_ITEMS);
  }

  writeState({ ...state, items: nextItems });
  return item;
}

export function updateItem(
  id: string,
  patch: Partial<Pick<QueryLibraryItem, 'title' | 'query' | 'folderId' | 'tags' | 'isFavorite'>>
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
    if (state.folders.length + importedFolders.length >= MAX_FOLDERS) break;
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
    if (state.items.length + importedItems.length >= MAX_ITEMS) break;
    const title = (item?.title ?? '').trim();
    const query = item?.query ?? '';
    if (!title || !query.trim()) continue;
    const folderId =
      item.folderId && folderIdMap.has(item.folderId)
        ? folderIdMap.get(item.folderId)
        : null;
    importedItems.push({
      id: generateId('ql'),
      title,
      query,
      folderId,
      tags: Array.from(new Set((item.tags ?? []).map(normalizeTag).filter(Boolean))).slice(0, 12),
      isFavorite: !!item.isFavorite,
      driver: item.driver,
      database: item.database,
      createdAt: now(),
      updatedAt: now(),
    });
  }

  writeState({
    folders: [...state.folders, ...importedFolders],
    items: [...importedItems, ...state.items].slice(0, MAX_ITEMS),
  });

  return { foldersImported: importedFolders.length, itemsImported: importedItems.length };
}

