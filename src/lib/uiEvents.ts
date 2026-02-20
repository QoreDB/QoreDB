// SPDX-License-Identifier: Apache-2.0

export const UI_EVENT_OPEN_LOGS = 'qoredb:open-logs';
export const UI_EVENT_OPEN_HISTORY = 'qoredb:open-history';
export const UI_EVENT_REFRESH_TABLE = 'qoredb:refresh-table';
export const UI_EVENT_EXPORT_DATA = 'qoredb:export-data';
export const UI_EVENT_CONNECTIONS_CHANGED = 'qoredb:connections-changed';

export type ExportDataDetail = {
  format?: 'csv' | 'json';
};

export function emitUiEvent<T>(name: string, detail?: T): void {
  if (typeof window === 'undefined') return;
  if (detail === undefined) {
    window.dispatchEvent(new Event(name));
    return;
  }
  window.dispatchEvent(new CustomEvent<T>(name, { detail }));
}
