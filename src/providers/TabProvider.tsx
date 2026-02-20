// SPDX-License-Identifier: Apache-2.0

import { createContext, type ReactNode, useContext } from 'react';
import type { DatabaseBrowserTab } from '@/components/Browser/DatabaseBrowser';
import type { TableBrowserTab } from '@/components/Browser/TableBrowser';
import { type UseTabsOptions, useTabs } from '@/hooks/useTabs';
import type { OpenTab } from '@/lib/tabs';
import type { Namespace } from '@/lib/tauri';

export interface TabContextValue {
  tabs: OpenTab[];
  activeTabId: string | null;
  activeTab: OpenTab | undefined;
  queryDrafts: Record<string, string>;
  tableBrowserTabs: Record<string, TableBrowserTab>;
  databaseBrowserTabs: Record<string, DatabaseBrowserTab>;
  openTab: (tab: OpenTab) => void;
  closeTab: (tabId: string) => void;
  setActiveTabId: (id: string | null) => void;
  updateQueryDraft: (tabId: string, value: string) => void;
  updateTabNamespace: (tabId: string, namespace: Namespace) => void;
  updateTableBrowserTab: (tabId: string, tab: TableBrowserTab) => void;
  updateDatabaseBrowserTab: (tabId: string, tab: DatabaseBrowserTab) => void;
  resetTabs: (options?: UseTabsOptions) => void;
}

const TabContext = createContext<TabContextValue | null>(null);

export function TabProvider({ children }: { children: ReactNode }) {
  const {
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
    reset,
  } = useTabs();

  return (
    <TabContext.Provider
      value={{
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
        resetTabs: reset,
      }}
    >
      {children}
    </TabContext.Provider>
  );
}

export function useTabContext(): TabContextValue {
  const ctx = useContext(TabContext);
  if (!ctx) throw new Error('useTabContext must be used within TabProvider');
  return ctx;
}
