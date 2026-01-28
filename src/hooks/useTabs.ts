import { useState, useCallback, useMemo, useRef } from 'react';
import { OpenTab } from '@/lib/tabs';
import { TableBrowserTab } from '@/components/Browser/TableBrowser';
import { DatabaseBrowserTab } from '@/components/Browser/DatabaseBrowser';

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
  const [queryDrafts, setQueryDrafts] = useState<Record<string, string>>(options.initialQueryDrafts ?? {});

  const tableBrowserTabsRef = useRef<Record<string, TableBrowserTab>>(
    options.initialTableBrowserTabs ?? {}
  );
  const databaseBrowserTabsRef = useRef<Record<string, DatabaseBrowserTab>>(
    options.initialDatabaseBrowserTabs ?? {}
  );

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

  const closeTab = useCallback((tabId: string) => {
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

    delete tableBrowserTabsRef.current[tabId];
    delete databaseBrowserTabsRef.current[tabId];
  }, []);

  const updateQueryDraft = useCallback((tabId: string, value: string) => {
    setQueryDrafts(prev => {
      if (prev[tabId] === value) return prev;
      return { ...prev, [tabId]: value };
    });
  }, []);

  const reset = useCallback((options: UseTabsOptions = {}) => {
    setTabs(options.initialTabs ?? []);
    setActiveTabId(options.initialActiveTabId ?? options.initialTabs?.[0]?.id ?? null);
    setQueryDrafts(options.initialQueryDrafts ?? {});
    tableBrowserTabsRef.current = options.initialTableBrowserTabs ?? {};
    databaseBrowserTabsRef.current = options.initialDatabaseBrowserTabs ?? {};
  }, []);

  return {
    tabs,
    activeTabId,
    activeTab,
    queryDrafts,
    tableBrowserTabsRef,
    databaseBrowserTabsRef,
    openTab,
    closeTab,
    setActiveTabId,
    updateQueryDraft,
    reset,
  };
}
