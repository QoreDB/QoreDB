// SPDX-License-Identifier: Apache-2.0

const STORAGE_KEY = 'qoredb_tabs_group_by_connection';

export function getGroupByConnection(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === '1';
  } catch {
    return false;
  }
}

export function setGroupByConnection(enabled: boolean): void {
  try {
    if (enabled) {
      localStorage.setItem(STORAGE_KEY, '1');
    } else {
      localStorage.removeItem(STORAGE_KEY);
    }
    window.dispatchEvent(new CustomEvent('qoredb:tabs-group-by-connection-changed'));
  } catch {
    /* ignore */
  }
}

export function subscribeGroupByConnection(handler: (enabled: boolean) => void): () => void {
  const listener = () => handler(getGroupByConnection());
  window.addEventListener('qoredb:tabs-group-by-connection-changed', listener);
  return () => window.removeEventListener('qoredb:tabs-group-by-connection-changed', listener);
}
