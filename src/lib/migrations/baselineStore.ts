// SPDX-License-Identifier: BUSL-1.1

//! Disk-backed schema baselines, one file per connection in `.qoredb/baselines/`.
//! A baseline is the reference schema used for drift detection and migration
//! generation. Files are keyed by the workspace-stable connection id and hold a
//! map of database -> snapshot, so a single connection used against several
//! databases keeps a baseline per database.

import { useSyncExternalStore } from 'react';
import { wsDeleteBaseline, wsReadBaseline, wsWriteBaseline } from '@/lib/tauri';
import type { SchemaSnapshot } from './schemaDiff';

const FILE_VERSION = 1 as const;

interface BaselineFile {
  version: typeof FILE_VERSION;
  baselines: Record<string, SchemaSnapshot>;
}

/** Parsed baseline file per connection id. Absent = not yet loaded from disk. */
const cache = new Map<string, BaselineFile>();
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

function emptyFile(): BaselineFile {
  return { version: FILE_VERSION, baselines: {} };
}

function dbKey(database?: string | null): string {
  return database ?? '';
}

function isSnapshot(value: unknown): value is SchemaSnapshot {
  return (
    typeof value === 'object' &&
    value !== null &&
    'tables' in value &&
    typeof (value as SchemaSnapshot).tables === 'object'
  );
}

function parseFile(raw: string): BaselineFile {
  try {
    const parsed = JSON.parse(raw) as Partial<BaselineFile>;
    if (parsed?.version !== FILE_VERSION || typeof parsed.baselines !== 'object') {
      return emptyFile();
    }
    // Drop malformed entries rather than trusting the whole file blindly.
    const baselines: Record<string, SchemaSnapshot> = {};
    for (const [key, snapshot] of Object.entries(parsed.baselines ?? {})) {
      if (isSnapshot(snapshot)) baselines[key] = snapshot;
    }
    return { version: FILE_VERSION, baselines };
  } catch {
    return emptyFile();
  }
}

function serialize(file: BaselineFile): string {
  return JSON.stringify(file, null, 2);
}

/** Loads a connection's baseline file from disk into the cache (idempotent). */
export async function loadBaselineFile(connectionId: string): Promise<void> {
  try {
    const raw = await wsReadBaseline(connectionId);
    cache.set(connectionId, raw ? parseFile(raw) : emptyFile());
  } catch (err) {
    console.warn('Failed to load baseline file:', err);
    cache.set(connectionId, emptyFile());
  }
  emit();
}

function getBaseline(connectionId: string, database?: string | null): SchemaSnapshot | null {
  return cache.get(connectionId)?.baselines[dbKey(database)] ?? null;
}

/** Persists a snapshot as the baseline for (connection, database). Throws on write failure. */
export async function saveBaseline(
  connectionId: string,
  database: string | null | undefined,
  snapshot: SchemaSnapshot
): Promise<void> {
  const current = cache.get(connectionId) ?? emptyFile();
  const next: BaselineFile = {
    version: FILE_VERSION,
    baselines: { ...current.baselines, [dbKey(database)]: snapshot },
  };
  const ok = await wsWriteBaseline(connectionId, serialize(next));
  if (!ok) throw new Error('workspace_required');
  cache.set(connectionId, next);
  emit();
}

/** Removes the baseline for (connection, database), deleting the file if it becomes empty. */
export async function clearBaseline(
  connectionId: string,
  database: string | null | undefined
): Promise<void> {
  const current = cache.get(connectionId);
  if (!current || !(dbKey(database) in current.baselines)) return;
  const baselines = { ...current.baselines };
  delete baselines[dbKey(database)];
  const next: BaselineFile = { version: FILE_VERSION, baselines };
  if (Object.keys(baselines).length === 0) {
    await wsDeleteBaseline(connectionId);
  } else {
    await wsWriteBaseline(connectionId, serialize(next));
  }
  cache.set(connectionId, next);
  emit();
}

export function useBaseline(
  connectionId: string | null,
  database?: string | null
): SchemaSnapshot | null {
  const read = () => (connectionId ? getBaseline(connectionId, database) : null);
  return useSyncExternalStore(
    listener => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    read,
    read
  );
}
