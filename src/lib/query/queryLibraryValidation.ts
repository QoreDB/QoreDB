// SPDX-License-Identifier: Apache-2.0

import i18n from '../../i18n';
import type { QueryFolder, QueryLibraryItem, QueryVariable } from './queryLibrary';

export interface QueryLibraryState {
  folders: QueryFolder[];
  items: QueryLibraryItem[];
  pendingSync?: boolean;
}

export function invalidLibrary(): never {
  throw new Error(i18n.t('library.invalidData'));
}

function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return invalidLibrary();
  return value as Record<string, unknown>;
}

function text(value: unknown): string {
  if (typeof value !== 'string' || !value.trim()) return invalidLibrary();
  return value;
}

function optionalText(value: unknown): void {
  if (value != null && typeof value !== 'string') invalidLibrary();
}

function timestamp(value: unknown): number {
  if (value == null) return 0;
  if (typeof value !== 'number' || !Number.isFinite(value)) return invalidLibrary();
  return value;
}

function strings(value: unknown): string[] {
  if (!Array.isArray(value) || value.some(item => typeof item !== 'string'))
    return invalidLibrary();
  return value;
}

function variables(value: unknown): Record<string, QueryVariable> {
  return Object.fromEntries(
    Object.entries(record(value)).map(([name, raw]) => {
      const variable = record(raw);
      if (!['text', 'number', 'date', 'select'].includes(String(variable.type))) invalidLibrary();
      for (const field of ['name', 'defaultValue', 'currentValue', 'description'])
        optionalText(variable[field]);
      if (variable.options != null) strings(variable.options);
      return [name, { ...variable, name: variable.name ?? name } as unknown as QueryVariable];
    })
  );
}

/** Validate before any mutation; optional legacy display fields get safe in-memory defaults. */
export function parseLibraryState(value: unknown, importing = false): QueryLibraryState {
  const source = record(value);
  if (
    (source.version !== undefined && source.version !== 1) ||
    !Array.isArray(source.folders) ||
    !Array.isArray(source.items) ||
    (source.pendingSync !== undefined && typeof source.pendingSync !== 'boolean')
  )
    invalidLibrary();
  const folderIds = new Set<string>();
  const folders = (source.folders as unknown[]).map(raw => {
    const folder = record(raw);
    const id = text(folder.id);
    if (folderIds.has(id)) invalidLibrary();
    folderIds.add(id);
    return {
      ...folder,
      id,
      name: text(folder.name),
      createdAt: timestamp(folder.createdAt),
      updatedAt: timestamp(folder.updatedAt),
    } as QueryFolder;
  });
  const itemIds = new Set<string>();
  const items = (source.items as unknown[]).map(raw => {
    const item = record(raw);
    const id = importing && item.id == null ? '' : text(item.id);
    if (id && itemIds.has(id)) invalidLibrary();
    if (id) itemIds.add(id);
    for (const field of ['folderId', 'driver', 'database']) optionalText(item[field]);
    if (importing && item.folderId && !folderIds.has(item.folderId as string)) invalidLibrary();
    if (item.isFavorite != null && typeof item.isFavorite !== 'boolean') invalidLibrary();
    return {
      ...item,
      id,
      title: text(item.title),
      query: text(item.query),
      tags: item.tags == null ? [] : strings(item.tags),
      isFavorite: item.isFavorite ?? false,
      createdAt: timestamp(item.createdAt),
      updatedAt: timestamp(item.updatedAt),
      ...(item.variables == null ? {} : { variables: variables(item.variables) }),
    } as QueryLibraryItem;
  });
  return { ...source, folders, items } as QueryLibraryState;
}

export function parseLibraryExport(value: unknown, importing = true): QueryLibraryState {
  if (record(value).version !== 1) return invalidLibrary();
  return parseLibraryState(value, importing);
}

export function parseStoredLibrary(raw: string): QueryLibraryState {
  try {
    return parseLibraryState(JSON.parse(raw));
  } catch {
    // JSON errors may include query text. Show only the translated generic error.
    return invalidLibrary();
  }
}
