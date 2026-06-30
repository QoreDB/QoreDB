// SPDX-License-Identifier: Apache-2.0

import { type ReactNode, useEffect, useEffectEvent } from 'react';
import { KeyboardCheatsheet } from '@/components/KeyboardCheatsheet';
import { useShortcutBindings } from '@/hooks/useKeyboardShortcuts';
import {
  emitUiEvent,
  UI_EVENT_REFRESH_TABLE,
  UI_EVENT_TOGGLE_SANDBOX,
} from '@/lib/events/uiEvents';
import { chordMatches, SHORTCUT_DEFINITIONS, type ShortcutId } from '@/lib/shortcuts';
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
  const bindings = useShortcutBindings();

  const handleKeyDown = useEffectEvent((e: KeyboardEvent) => {
    const modal = getModalState();
    const isOverlayOpen =
      modal.searchOpen ||
      modal.fulltextSearchOpen ||
      modal.connectionModalOpen ||
      modal.libraryModalOpen ||
      modal.logsOpen ||
      modal.auditLogOpen;

    // Block window-level shortcuts while a dialog overlay is open.
    if (isOverlayOpen) return;

    const inTextInput = isTextInputTarget(e.target);

    // Find the first definition whose chord matches the event.
    const triggered = SHORTCUT_DEFINITIONS.find(def => chordMatches(e, bindings[def.id]));
    if (!triggered) return;

    if (inTextInput && !triggered.worksInTextInput && triggered.id !== 'escape') {
      return;
    }

    e.preventDefault();
    dispatch(triggered.id);
  });

  function dispatch(id: ShortcutId) {
    const modal = getModalState();
    switch (id) {
      case 'search':
        setSearchOpen(true);
        break;
      case 'settings':
        setSettingsOpen(true);
        break;
      case 'cheatsheet':
        toggleCheatsheet();
        break;
      case 'escape':
        if (modal.zenMode) {
          setZenMode(false);
        } else if (activeTabId) {
          closeTab(activeTabId);
        } else if (modal.settingsOpen) {
          setSettingsOpen(false);
        }
        break;
      case 'newQuery':
        if (sessionId) {
          openTab(createQueryTab(undefined, activeTab?.namespace));
        }
        break;
      case 'closeTab':
        if (activeTabId) closeTab(activeTabId);
        break;
      case 'newConnection':
        setConnectionModalOpen(true);
        break;
      case 'convertToNotebook':
        if (sessionId && activeTab?.type === 'query') {
          const draft = queryDrafts[activeTab.id] ?? '';
          const tab = createNotebookTab(undefined, undefined, draft);
          tab.namespace = activeTab.namespace;
          openTab(tab);
        }
        break;
      case 'openLibrary':
        setLibraryModalOpen(true);
        break;
      case 'fulltextSearch':
        if (sessionId) setFulltextSearchOpen(true);
        break;
      case 'refreshData':
        if (activeTab?.type === 'table') emitUiEvent(UI_EVENT_REFRESH_TABLE);
        break;
      case 'toggleSandbox':
        if (sessionId) emitUiEvent(UI_EVENT_TOGGLE_SANDBOX);
        break;
    }
  }

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
