// SPDX-License-Identifier: Apache-2.0

import type { Namespace } from './tauri';

export type TableChangeType = 'create' | 'truncate' | 'drop' | 'update';

export interface TableChangeEvent {
  type: TableChangeType;
  namespace: Namespace;
  tableName: string;
}

const TABLE_CHANGE_EVENT = 'qoredb-table-change';

export function emitTableChange(event: TableChangeEvent): void {
  if (typeof window === 'undefined') return;
  window.dispatchEvent(new CustomEvent<TableChangeEvent>(TABLE_CHANGE_EVENT, { detail: event }));
}

export function onTableChange(handler: (event: TableChangeEvent) => void): () => void {
  if (typeof window === 'undefined') return () => {};
  const listener = (event: Event) => {
    const detail = (event as CustomEvent<TableChangeEvent>).detail;
    if (detail) {
      handler(detail);
    }
  };
  window.addEventListener(TABLE_CHANGE_EVENT, listener);
  return () => window.removeEventListener(TABLE_CHANGE_EVENT, listener);
}
