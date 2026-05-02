// SPDX-License-Identifier: Apache-2.0

import { useSyncExternalStore } from 'react';

interface TransactionState {
  active: boolean;
  statementCount: number;
}

let state: TransactionState = {
  active: false,
  statementCount: 0,
};

const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

export function setTransactionActive(active: boolean) {
  state = { active, statementCount: active ? 0 : 0 };
  emit();
}

export function incrementTransactionStatements() {
  if (!state.active) return;
  state = { ...state, statementCount: state.statementCount + 1 };
  emit();
}

export function resetTransactionState() {
  state = { active: false, statementCount: 0 };
  emit();
}

export function useTransactionStore(): TransactionState {
  return useSyncExternalStore(
    cb => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    () => state
  );
}
