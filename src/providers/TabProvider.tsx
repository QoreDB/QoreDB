// SPDX-License-Identifier: Apache-2.0

import { createContext, type ReactNode, useContext, useMemo } from 'react';
import type { DatabaseBrowserTab } from '@/components/Browser/DatabaseBrowser';
import type { TableBrowserTab } from '@/components/Browser/TableBrowser';
import { type BeforeCloseTabHandler, type UseTabsOptions, useTabs } from '@/hooks/useTabs';
import type { OpenTab } from '@/lib/tabs';
import type { Namespace } from '@/lib/tauri';

/**
 * Tab state that changes frequently (on every keystroke in the SQL editor,
 * every tab switch, every browser state update). Consumers of this context
 * re-render on each of those events.
 */
export interface TabStateValue {
  tabs: OpenTab[];
  activeTabId: string | null;
  activeTab: OpenTab | undefined;
  queryDrafts: Record<string, string>;
  tableBrowserTabs: Record<string, TableBrowserTab>;
  databaseBrowserTabs: Record<string, DatabaseBrowserTab>;
}

/**
 * Tab mutation actions. All callbacks are stable refs (wrapped in useCallback
 * in `useTabs`), so this context value is mounted once and never changes.
 * Consumers reading *only* actions (e.g. a button that calls `openTab`) do
 * not re-render when tab state changes.
 */
export interface TabActionsValue {
  openTab: (tab: OpenTab) => void;
  closeTab: (tabId: string) => Promise<void> | void;
  setActiveTabId: (id: string | null) => void;
  updateQueryDraft: (tabId: string, value: string) => void;
  updateTabNamespace: (tabId: string, namespace: Namespace) => void;
  updateTableBrowserTab: (tabId: string, tab: TableBrowserTab) => void;
  updateDatabaseBrowserTab: (tabId: string, tab: DatabaseBrowserTab) => void;
  updateTab: (tabId: string, updates: Partial<OpenTab>) => void;
  reorderTabs: (newTabs: OpenTab[]) => void;
  togglePinTab: (tabId: string) => void;
  setBeforeCloseTab: (handler: BeforeCloseTabHandler | null) => void;
  resetTabs: (options?: UseTabsOptions) => void;
}

/** Combined shape preserved for backward compatibility with `useTabContext`. */
export interface TabContextValue extends TabStateValue, TabActionsValue {}

const TabStateContext = createContext<TabStateValue | null>(null);
const TabActionsContext = createContext<TabActionsValue | null>(null);

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
    updateTab,
    reorderTabs,
    togglePinTab,
    setBeforeCloseTab,
    reset,
  } = useTabs();

  const stateValue = useMemo<TabStateValue>(
    () => ({
      tabs,
      activeTabId,
      activeTab,
      queryDrafts,
      tableBrowserTabs,
      databaseBrowserTabs,
    }),
    [tabs, activeTabId, activeTab, queryDrafts, tableBrowserTabs, databaseBrowserTabs]
  );

  const actionsValue = useMemo<TabActionsValue>(
    () => ({
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
      resetTabs: reset,
    }),
    [
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
    ]
  );

  return (
    <TabActionsContext.Provider value={actionsValue}>
      <TabStateContext.Provider value={stateValue}>{children}</TabStateContext.Provider>
    </TabActionsContext.Provider>
  );
}

/** Subscribe to tab state only. Re-renders on tab/draft/browser changes. */
export function useTabState(): TabStateValue {
  const ctx = useContext(TabStateContext);
  if (!ctx) throw new Error('useTabState must be used within TabProvider');
  return ctx;
}

/** Subscribe to tab mutation actions only. Never re-renders — callbacks are stable. */
export function useTabActions(): TabActionsValue {
  const ctx = useContext(TabActionsContext);
  if (!ctx) throw new Error('useTabActions must be used within TabProvider');
  return ctx;
}

/**
 * Legacy combined hook — subscribes to BOTH contexts. Prefer `useTabState` or
 * `useTabActions` in new code to avoid unnecessary re-renders.
 */
export function useTabContext(): TabContextValue {
  const state = useTabState();
  const actions = useTabActions();
  return useMemo(() => ({ ...state, ...actions }), [state, actions]);
}
