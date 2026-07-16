// SPDX-License-Identifier: Apache-2.0

import { useSyncExternalStore } from 'react';
import { wsListMigrations } from '@/lib/tauri';
import type { MigrationSummary } from './types';

interface MigrationsState {
  /** null = default workspace (migrations are file-based only) or not loaded yet. */
  migrations: MigrationSummary[] | null;
  isLoading: boolean;
}

let state: MigrationsState = {
  migrations: null,
  isLoading: false,
};

const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

function setState(patch: Partial<MigrationsState>) {
  state = { ...state, ...patch };
  emit();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Fetches the migration list from disk into the store. */
export async function loadMigrations(): Promise<void> {
  setState({ isLoading: true });
  try {
    const migrations = await wsListMigrations();
    setState({ migrations, isLoading: false });
  } catch (err) {
    console.warn('Failed to load migrations:', err);
    setState({ isLoading: false });
  }
}

export function useMigrationsStore<T>(selector: (state: MigrationsState) => T): T {
  return useSyncExternalStore(
    subscribe,
    () => selector(state),
    () => selector(state)
  );
}
