// SPDX-License-Identifier: Apache-2.0

import type { Update } from '@tauri-apps/plugin-updater';
import { useSyncExternalStore } from 'react';

export type UpdateStatus = 'idle' | 'available' | 'installing' | 'installed' | 'error';

interface UpdateState {
  status: UpdateStatus;
  version: string | null;
  update: Update | null;
  error: string | null;
}

let state: UpdateState = {
  status: 'idle',
  version: null,
  update: null,
  error: null,
};

const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

export function setUpdateAvailable(update: Update) {
  state = { status: 'available', version: update.version, update, error: null };
  emit();
}

export function setUpdateInstalling() {
  state = { ...state, status: 'installing', error: null };
  emit();
}

export function setUpdateInstalled() {
  state = { ...state, status: 'installed', error: null };
  emit();
}

export function setUpdateError(error: string) {
  state = { ...state, status: 'error', error };
  emit();
}

export function clearUpdate() {
  state = { status: 'idle', version: null, update: null, error: null };
  emit();
}

export function useUpdateStore(): UpdateState {
  return useSyncExternalStore(
    cb => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    () => state
  );
}
