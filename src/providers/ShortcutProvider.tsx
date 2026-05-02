// SPDX-License-Identifier: Apache-2.0

import { type ReactNode, useEffect, useEffectEvent } from 'react';
import { KeyboardCheatsheet } from '@/components/KeyboardCheatsheet';
import {
  getModalState,
  setCheatsheetOpen,
  setConnectionModalOpen,
  setFulltextSearchOpen,
  setLibraryModalOpen,
  setSearchOpen,
  setSettingsOpen,
  setZenMode,
  toggleCheatsheet,
  useModalStore,
} from '@/lib/stores/modalStore';
import { createNotebookTab, createQueryTab } from '@/lib/tabs';
import { useSessionContext } from './SessionProvider';
import { useTabContext } from './TabProvider';

function isTextInputTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName.toLowerCase();
  return tag === 'input' || tag === 'textarea' || tag === 'select' || target.isContentEditable;
}

export function ShortcutProvider({ children }: { children: ReactNode }) {
  const { activeTabId, activeTab, closeTab, openTab, queryDrafts } = useTabContext();
  const { sessionId } = useSessionContext();
  const cheatsheetOpen = useModalStore(s => s.cheatsheetOpen);

  const handleKeyDown = useEffectEvent((e: KeyboardEvent) => {
    const modal = getModalState();
    const isOverlayOpen =
      modal.searchOpen ||
      modal.fulltextSearchOpen ||
      modal.connectionModalOpen ||
      modal.libraryModalOpen ||
      modal.logsOpen;

    // Block all shortcuts while a dialog overlay is open.
    // Escape is handled by Radix Dialog itself (stopPropagation prevents it from reaching here).
    if (isOverlayOpen) {
      return;
    }

    // Mod+K always opens search, even in text inputs
    if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
      e.preventDefault();
      setSearchOpen(true);
      return;
    }

    // Mod+Shift+N: Convert query to notebook (works even in text inputs)
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'n') {
      e.preventDefault();
      if (sessionId && activeTab?.type === 'query') {
        const draft = queryDrafts[activeTab.id] ?? '';
        const tab = createNotebookTab(undefined, undefined, draft);
        tab.namespace = activeTab.namespace;
        openTab(tab);
      }
      return;
    }

    // Skip other shortcuts when in text input
    if (isTextInputTarget(e.target)) {
      return;
    }

    // ?: Toggle keyboard cheatsheet
    if (e.key === '?') {
      e.preventDefault();
      toggleCheatsheet();
      return;
    }

    // Mod+N: New connection
    if ((e.metaKey || e.ctrlKey) && e.key === 'n') {
      e.preventDefault();
      setConnectionModalOpen(true);
      return;
    }

    // Mod+Shift+L: Open library
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'l') {
      e.preventDefault();
      setLibraryModalOpen(true);
      return;
    }

    // Mod+Shift+F: Open full-text search
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      if (sessionId) {
        setFulltextSearchOpen(true);
      }
      return;
    }

    // Mod+,: Settings
    if ((e.metaKey || e.ctrlKey) && e.key === ',') {
      e.preventDefault();
      setSettingsOpen(true);
      return;
    }

    // Escape: Exit zen mode first, then close active tab or settings
    if (e.key === 'Escape') {
      if (modal.zenMode) {
        setZenMode(false);
        return;
      }
      if (activeTabId) closeTab(activeTabId);
      else if (modal.settingsOpen) setSettingsOpen(false);
      return;
    }

    // Mod+W: Close active tab
    if ((e.metaKey || e.ctrlKey) && e.key === 'w') {
      e.preventDefault();
      if (activeTabId) {
        closeTab(activeTabId);
      }
      return;
    }

    // Mod+T: New query tab
    if ((e.metaKey || e.ctrlKey) && e.key === 't') {
      e.preventDefault();
      if (sessionId) {
        openTab(createQueryTab(undefined, activeTab?.namespace));
      }
    }
  });

  useEffect(() => {
    function onWindowKeyDown(e: KeyboardEvent) {
      handleKeyDown(e);
    }

    window.addEventListener('keydown', onWindowKeyDown);
    return () => window.removeEventListener('keydown', onWindowKeyDown);
  }, []);

  return (
    <>
      {children}
      <KeyboardCheatsheet open={cheatsheetOpen} onClose={() => setCheatsheetOpen(false)} />
    </>
  );
}
