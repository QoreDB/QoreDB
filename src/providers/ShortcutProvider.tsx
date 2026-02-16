import { useCallback, type ReactNode } from 'react';
import { useKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts';
import { createQueryTab } from '@/lib/tabs';
import { useTabContext } from './TabProvider';
import { useSessionContext } from './SessionProvider';
import { useModalContext } from './ModalProvider';

export function ShortcutProvider({ children }: { children: ReactNode }) {
  const { activeTabId, activeTab, closeTab, openTab } = useTabContext();
  const { sessionId } = useSessionContext();
  const {
    searchOpen,
    fulltextSearchOpen,
    connectionModalOpen,
    libraryModalOpen,
    settingsOpen,
    setSearchOpen,
    setConnectionModalOpen,
    setLibraryModalOpen,
    setFulltextSearchOpen,
    setSettingsOpen,
  } = useModalContext();

  const handleNewQuery = useCallback(() => {
    if (sessionId) {
      openTab(createQueryTab(undefined, activeTab?.namespace));
    }
  }, [sessionId, openTab, activeTab?.namespace]);

  const handleEscape = useCallback(() => {
    if (searchOpen) setSearchOpen(false);
    else if (fulltextSearchOpen) setFulltextSearchOpen(false);
    else if (connectionModalOpen) setConnectionModalOpen(false);
    else if (libraryModalOpen) setLibraryModalOpen(false);
    else if (activeTabId) closeTab(activeTabId);
    else if (settingsOpen) setSettingsOpen(false);
  }, [
    searchOpen,
    fulltextSearchOpen,
    connectionModalOpen,
    libraryModalOpen,
    settingsOpen,
    activeTabId,
    setSearchOpen,
    setFulltextSearchOpen,
    setConnectionModalOpen,
    setLibraryModalOpen,
    setSettingsOpen,
    closeTab,
  ]);

  useKeyboardShortcuts({
    onSearch: () => setSearchOpen(true),
    onNewConnection: () => setConnectionModalOpen(true),
    onOpenLibrary: () => setLibraryModalOpen(true),
    onFulltextSearch: () => sessionId && setFulltextSearchOpen(true),
    onSettings: () => setSettingsOpen(true),
    onCloseTab: () => activeTabId && closeTab(activeTabId),
    onNewQuery: handleNewQuery,
    onEscape: handleEscape,
    isOverlayOpen: searchOpen || fulltextSearchOpen || connectionModalOpen || libraryModalOpen,
    hasSession: Boolean(sessionId),
    hasActiveTab: Boolean(activeTabId),
  });

  return <>{children}</>;
}
