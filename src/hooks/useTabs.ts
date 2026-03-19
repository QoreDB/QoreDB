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

  const openTab = useCallback((tab: OpenTab) => {
    setTabs(prev => {
      // For non-query tabs, check if already exists
      const existing =
        tab.type === 'query'
          ? undefined
          : prev.find(
              t =>
                t.type === tab.type &&
                t.namespace?.database === tab.namespace?.database &&
                t.namespace?.schema === tab.namespace?.schema &&
                t.tableName === tab.tableName
            );

      if (existing) {
        setActiveTabId(existing.id);
        return prev.map(t =>
          t.id === existing.id
            ? {
                ...t,
                title: tab.title,
                namespace: tab.namespace,
                tableName: tab.tableName,
                relationFilter: tab.relationFilter,
                searchFilter: tab.searchFilter,
              }
            : t
        );
      }

      setActiveTabId(tab.id);
      return [...prev, tab];
    });

    if (tab.type === 'query' && tab.initialQuery) {
      setQueryDrafts(prev => (prev[tab.id] ? prev : { ...prev, [tab.id]: tab.initialQuery || '' }));
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
    reset,
  };
}
