// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { DatabaseBrowserTab } from '@/components/Browser/DatabaseBrowser';
import type { TableBrowserTab } from '@/components/Browser/TableBrowser';
import {
  type CrashRecoverySnapshot,
  clearCrashRecoverySnapshot,
  getCrashRecoverySnapshot,
  saveCrashRecoverySnapshot,
} from '@/lib/crashRecovery';
import type { OpenTab } from '@/lib/tabs';
import { connectSavedConnection, listSavedConnections, type SavedConnection } from '@/lib/tauri';

const DEFAULT_PROJECT = 'default';

function sanitizeTableBrowserTabs(input?: Record<string, string>): Record<string, TableBrowserTab> {
  const result: Record<string, TableBrowserTab> = {};
  if (!input) return result;
  for (const [id, tab] of Object.entries(input)) {
    if (tab === 'data' || tab === 'structure' || tab === 'info') {
      result[id] = tab;
    }
  }
  return result;
}

function sanitizeDatabaseBrowserTabs(
  input?: Record<string, string>
): Record<string, DatabaseBrowserTab> {
  const result: Record<string, DatabaseBrowserTab> = {};
  if (!input) return result;
  for (const [id, tab] of Object.entries(input)) {
    if (tab === 'overview' || tab === 'tables' || tab === 'schema') {
      result[id] = tab;
    }
  }
  return result;
}

export interface RecoveryState {
  snapshot: CrashRecoverySnapshot | null;
  connectionName: string | null;
  isMissing: boolean;
  isLoading: boolean;
  error: string | null;
}

export interface RestoredSession {
  sessionId: string;
  connection: SavedConnection;
  tabs: OpenTab[];
  activeTabId: string | null;
  queryDrafts: Record<string, string>;
  tableBrowserTabs: Record<string, TableBrowserTab>;
  databaseBrowserTabs: Record<string, DatabaseBrowserTab>;
}

export function useRecovery() {
  const { t } = useTranslation();
  const [state, setState] = useState<RecoveryState>({
    snapshot: null,
    connectionName: null,
    isMissing: false,
    isLoading: false,
    error: null,
  });

  // Load recovery snapshot on mount
  useEffect(() => {
    const snapshot = getCrashRecoverySnapshot();
    if (!snapshot) return;

    setState(prev => ({ ...prev, snapshot }));

    listSavedConnections(DEFAULT_PROJECT)
      .then(saved => {
        const match = saved.find(conn => conn.id === snapshot.connectionId);
        setState(prev => ({
          ...prev,
          connectionName: match?.name ?? null,
          isMissing: !match,
        }));
      })
      .catch(() => {
        setState(prev => ({
          ...prev,
          connectionName: null,
          isMissing: true,
        }));
      });
  }, []);

  const restore = useCallback(async (): Promise<RestoredSession | null> => {
    if (!state.snapshot) return null;

    setState(prev => ({ ...prev, isLoading: true, error: null }));

    try {
      const saved = await listSavedConnections(DEFAULT_PROJECT);
      const match = saved.find(conn => conn.id === state.snapshot!.connectionId);

      if (!match) {
        setState(prev => ({
          ...prev,
          isMissing: true,
          error: t('recovery.missingConnection'),
          isLoading: false,
        }));
        return null;
      }

      const result = await connectSavedConnection(DEFAULT_PROJECT, match.id);
      if (!result.success || !result.session_id) {
        setState(prev => ({
          ...prev,
          error: result.error || t('recovery.restoreFailed'),
          isLoading: false,
        }));
        return null;
      }

      const restoredTabs: OpenTab[] = state.snapshot!.tabs.map(tab => {
        const restored: OpenTab = {
          id: tab.id,
          type: tab.type,
          title: tab.title,
          namespace: tab.namespace,
          tableName: tab.tableName,
        };

        if (tab.type === 'query') {
          const query = state.snapshot!.queryDrafts[tab.id];
          if (query) {
            restored.initialQuery = query;
          }
        }

        return restored;
      });

      // Clear recovery after successful restore
      clearCrashRecoverySnapshot();
      setState({
        snapshot: null,
        connectionName: null,
        isMissing: false,
        isLoading: false,
        error: null,
      });

      return {
        sessionId: result.session_id,
        connection: {
          ...match,
          environment: match.environment,
          read_only: match.read_only,
        },
        tabs: restoredTabs,
        activeTabId: state.snapshot!.activeTabId,
        queryDrafts: state.snapshot!.queryDrafts,
        tableBrowserTabs: sanitizeTableBrowserTabs(state.snapshot!.tableBrowserTabs),
        databaseBrowserTabs: sanitizeDatabaseBrowserTabs(state.snapshot!.databaseBrowserTabs),
      };
    } catch (err) {
      setState(prev => ({
        ...prev,
        error: err instanceof Error ? err.message : t('common.unknownError'),
        isLoading: false,
      }));
      return null;
    }
  }, [state.snapshot, t]);

  const discard = useCallback(() => {
    clearCrashRecoverySnapshot();
    setState({
      snapshot: null,
      connectionName: null,
      isMissing: false,
      isLoading: false,
      error: null,
    });
  }, []);

  return {
    state,
    restore,
    discard,
    saveSnapshot: saveCrashRecoverySnapshot,
  };
}
