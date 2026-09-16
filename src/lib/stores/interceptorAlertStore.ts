// SPDX-License-Identifier: Apache-2.0

import { useSyncExternalStore } from 'react';
import type { InterceptorAlert } from '../tauri/interceptor';

interface InterceptorAlertState {
  /** Most recent alerts pushed by the backend, newest first. */
  alerts: InterceptorAlert[];
  /** Fingerprints whose P95 regressed, from the last trends poll. */
  regressions: number;
}

const MAX_ALERTS = 20;

let state: InterceptorAlertState = { alerts: [], regressions: 0 };
const listeners = new Set<() => void>();

function update(patch: Partial<InterceptorAlertState>) {
  state = { ...state, ...patch };
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function pushInterceptorAlert(alert: InterceptorAlert) {
  update({ alerts: [alert, ...state.alerts].slice(0, MAX_ALERTS) });
}

export function setRegressionCount(regressions: number) {
  if (state.regressions === regressions) return;
  update({ regressions });
}

export function clearInterceptorAlerts() {
  if (state.alerts.length === 0) return;
  update({ alerts: [] });
}

export function useInterceptorAlerts(): InterceptorAlertState {
  return useSyncExternalStore(subscribe, () => state);
}
