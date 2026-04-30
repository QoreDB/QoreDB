// SPDX-License-Identifier: Apache-2.0

import { useCallback, useMemo, useRef, useState } from 'react';
import type { DatabaseBrowserTab } from '@/components/Browser/DatabaseBrowser';
import type { TableBrowserTab } from '@/components/Browser/TableBrowser';
import type { OpenTab } from '@/lib/tabs';
import type { Namespace } from '@/lib/tauri';

export type BeforeCloseTabHandler = (tabId: string) => Promise<boolean> | boolean;

export interface UseTabsOptions {
  initialTabs?: OpenTab[];
  initialActiveTabId?: string | null;
  initialQueryDrafts?: Record<string, string>;
  initialTableBrowserTabs?: Record<string, TableBrowserTab>;
  initialDatabaseBrowserTabs?: Record<string, DatabaseBrowserTab>;
  /** Connection id stamped on tabs created via openTab. Tabs that already carry a connectionId are not overridden. */
  currentConnectionId?: string | null;
}

export function useTabs(options: UseTabsOptions = {}) {
  const [tabs, setTabs] = useState<OpenTab[]>(options.initialTabs ?? []);
  const [activeTabId, setActiveTabId] = useState<string | null>(options.initialActiveTabId ?? null);
  const [queryDrafts, setQueryDrafts] = useState<Record<string, string>>(
    options.initialQueryDrafts ?? {}
  );
  const [tableBrowserTabs, setTableBrowserTabs] = useState<Record<string, TableBrowserTab>>(
    options.initialTableBrowserTabs ?? {}
  );
  const [databaseBrowserTabs, setDatabaseBrowserTabs] = useState<
    Record<string, DatabaseBrowserTab>
  >(options.initialDatabaseBrowserTabs ?? {});

  const activeTab = useMemo(() => tabs.find(t => t.id === activeTabId), [tabs, activeTabId]);

  const currentConnectionIdRef = useRef<string | null>(options.currentConnectionId ?? null);

  const setCurrentConnectionId = useCallback((connectionId: string | null) => {
    currentConnectionIdRef.current = connectionId;
  }, []);

  const openTab = useCallback((tab: OpenTab) => {
    const stamped: OpenTab = {
      ...tab,
      connectionId: tab.connectionId ?? currentConnectionIdRef.current ?? undefined,
    };
    setTabs(prev => {
      // For non-query tabs, check if already exists
      const existing =
        stamped.type === 'query'
          ? undefined
          : prev.find(
              t =>
                t.type === stamped.type &&
                t.namespace?.database === stamped.namespace?.database &&
                t.namespace?.schema === stamped.namespace?.schema &&
                t.tableName === stamped.tableName &&
                t.connectionId === stamped.connectionId
            );

      if (existing) {
        setActiveTabId(existing.id);
        return prev.map(t =>
          t.id === existing.id
            ? {
                ...t,
                title: stamped.title,
                namespace: stamped.namespace,
                tableName: stamped.tableName,
                relationFilter: stamped.relationFilter,
                searchFilter: stamped.searchFilter,
              }
            : t
        );
      }

      setActiveTabId(stamped.id);
      return [...prev, stamped];
    });

    if (stamped.type === 'query' && stamped.initialQuery) {
      setQueryDrafts(prev =>
        prev[stamped.id] ? prev : { ...prev, [stamped.id]: stamped.initialQuery || '' }
      );
    }
  }, []);

  const beforeCloseTabRef = useRef<BeforeCloseTabHandler | null>(null);

  const setBeforeCloseTab = useCallback((handler: BeforeCloseTabHandler | null) => {
    beforeCloseTabRef.current = handler;
  }, []);

  const doCloseTab = useCallback((tabId: string) => {
    setTabs(prev => {
      const newTabs = prev.filter(t => t.id !== tabId);

      setActiveTabId(currentActiveId => {
        if (currentActiveId === tabId) {
          const closedIndex = prev.findIndex(t => t.id === tabId);
          const newActiveTab = newTabs[closedIndex] || newTabs[closedIndex - 1] || null;
          return newActiveTab?.id || null;
        }
        return currentActiveId;
      });

      return newTabs;
    });

    setQueryDrafts(prev => {
      if (!(tabId in prev)) return prev;
      const next = { ...prev };
      delete next[tabId];
      return next;
    });

    setTableBrowserTabs(prev => {
      if (!(tabId in prev)) return prev;
      const next = { ...prev };
      delete next[tabId];
      return next;
    });

    setDatabaseBrowserTabs(prev => {
      if (!(tabId in prev)) return prev;
      const next = { ...prev };
      delete next[tabId];
      return next;
    });
  }, []);

  const closeTab = useCallback(
    async (tabId: string) => {
      if (beforeCloseTabRef.current) {
        const allowed = await beforeCloseTabRef.current(tabId);
        if (!allowed) return;
      }
      doCloseTab(tabId);
    },
    [doCloseTab]
  );

  const updateTab = useCallback((tabId: string, updates: Partial<OpenTab>) => {
    setTabs(prev => prev.map(t => (t.id === tabId ? { ...t, ...updates } : t)));
  }, []);

  const updateQueryDraft = useCallback((tabId: string, value: string) => {
    setQueryDrafts(prev => {
      if (prev[tabId] === value) return prev;
      return { ...prev, [tabId]: value };
    });
  }, []);

  const updateTableBrowserTab = useCallback((tabId: string, tab: TableBrowserTab) => {
    setTableBrowserTabs(prev => {
      if (prev[tabId] === tab) return prev;
      return { ...prev, [tabId]: tab };
    });
  }, []);

  const updateDatabaseBrowserTab = useCallback((tabId: string, tab: DatabaseBrowserTab) => {
    setDatabaseBrowserTabs(prev => {
      if (prev[tabId] === tab) return prev;
      return { ...prev, [tabId]: tab };
    });
  }, []);

  const updateTabNamespace = useCallback((tabId: string, namespace: Namespace) => {
    setTabs(prev =>
      prev.map(t =>
        t.id === tabId &&
        (t.namespace?.database !== namespace.database || t.namespace?.schema !== namespace.schema)
          ? { ...t, namespace }
          : t
      )
    );
  }, []);

  const reorderTabs = useCallback((newTabs: OpenTab[]) => {
    setTabs(newTabs);
  }, []);

  const togglePinTab = useCallback((tabId: string) => {
    setTabs(prev => {
      const updated = prev.map(t => (t.id === tabId ? { ...t, pinned: !t.pinned } : t));
      // Sort: pinned tabs first, preserving relative order within each group
      const pinned = updated.filter(t => t.pinned);
      const unpinned = updated.filter(t => !t.pinned);
      return [...pinned, ...unpinned];
    });
  }, []);

  const reset = useCallback((options: UseTabsOptions = {}) => {
    setTabs(options.initialTabs ?? []);
    setActiveTabId(options.initialActiveTabId ?? options.initialTabs?.[0]?.id ?? null);
    setQueryDrafts(options.initialQueryDrafts ?? {});
    setTableBrowserTabs(options.initialTableBrowserTabs ?? {});
    setDatabaseBrowserTabs(options.initialDatabaseBrowserTabs ?? {});
    if (options.currentConnectionId !== undefined) {
      currentConnectionIdRef.current = options.currentConnectionId;
    }
  }, []);

  return {
    tabs,
    activeTabId,
    activeTab,
    queryDrafts,
    tableBrowserTabs,
    databaseBrowserTabs,
    openTab,
    closeTab,
    setActiveTabId,
    updateQueryDraft,
    updateTabNamespace,
    updateTableBrowserTab,
    updateDatabaseBrowserTab,
    updateTab,
    reorderTabs,
    togglePinTab,
    setBeforeCloseTab,
    setCurrentConnectionId,
    reset,
  };
}
