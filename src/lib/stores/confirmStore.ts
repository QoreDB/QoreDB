// SPDX-License-Identifier: Apache-2.0

import { useSyncExternalStore } from 'react';

export interface ConfirmOptions {
  title?: string;
  description: string;
  confirmLabel?: string;
  confirmationLabel?: string;
  warningInfo?: string;
}

interface ConfirmState {
  open: boolean;
  options: ConfirmOptions | null;
}

let state: ConfirmState = { open: false, options: null };
let resolver: ((confirmed: boolean) => void) | null = null;
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/**
 * Promisified replacement for window.confirm() backed by the design-system
 * DangerConfirmDialog. Resolves true on confirm, false on cancel/dismiss.
 */
export function confirmDialog(options: ConfirmOptions): Promise<boolean> {
  // A new request while one is pending dismisses the previous as cancelled.
  resolver?.(false);
  return new Promise<boolean>(resolve => {
    resolver = resolve;
    state = { open: true, options };
    emit();
  });
}

export function resolveConfirm(confirmed: boolean): void {
  resolver?.(confirmed);
  resolver = null;
  state = { open: false, options: state.options };
  emit();
}

export function useConfirmState(): ConfirmState {
  return useSyncExternalStore(
    subscribe,
    () => state,
    () => state
  );
}
