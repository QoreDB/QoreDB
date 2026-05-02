// SPDX-License-Identifier: Apache-2.0

export type TabBarEnvironment = 'development' | 'staging' | 'production';

export type TabKind =
  | 'query'
  | 'table'
  | 'database'
  | 'settings'
  | 'diff'
  | 'federation'
  | 'snapshots'
  | 'notebook'
  | 'time-travel';

export interface TabItem {
  id: string;
  title: string;
  pinned?: boolean;
  type: TabKind;
  connectionId?: string;
}

export interface ConnectionLabel {
  name: string;
  environment?: TabBarEnvironment;
}

export type ConnectionLabelLookup = (connectionId: string) => ConnectionLabel | undefined;
