// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef } from 'react';

function isTextInputTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName.toLowerCase();
  return tag === 'input' || tag === 'textarea' || tag === 'select' || target.isContentEditable;
}

export interface KeyboardShortcutsConfig {
  onSearch: () => void;
  onNewConnection: () => void;
  onOpenLibrary: () => void;
  onFulltextSearch: () => void;
  onSettings: () => void;
  onCloseTab: () => void;
  onNewQuery: () => void;
  onConvertToNotebook: () => void;
  onEscape: () => void;
  onShowCheatsheet?: () => void;
  isOverlayOpen: boolean;
  hasSession: boolean;
  hasActiveTab: boolean;
}

export function useKeyboardShortcuts(config: KeyboardShortcutsConfig) {
  // Store callbacks in a ref so the keydown listener always sees fresh values
  // without needing to re-register on every render.
  const configRef = useRef(config);
  configRef.current = config;

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const {
        onSearch,
        onNewConnection,
        onOpenLibrary,
        onFulltextSearch,
        onSettings,
        onCloseTab,
        onNewQuery,
        onConvertToNotebook,
        onEscape,
        onShowCheatsheet,
        isOverlayOpen,
        hasSession,
        hasActiveTab,
      } = configRef.current;

      // Handle overlay escape
      if (isOverlayOpen) {
        if (e.key === 'Escape') {
          e.preventDefault();
          onEscape();
        }
        return;
      }

      // Mod+K always opens search, even in text inputs
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        onSearch();
        return;
      }

      // Mod+Shift+N: Convert query to notebook (works even in text inputs)
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'n') {
        e.preventDefault();
        onConvertToNotebook();
        return;
      }

      // Skip other shortcuts when in text input
      if (isTextInputTarget(e.target)) {
        return;
      }

      // ?: Show keyboard cheatsheet
      if (e.key === '?' && onShowCheatsheet) {
        e.preventDefault();
        onShowCheatsheet();
        return;
      }

      // Mod+N: New connection
      if ((e.metaKey || e.ctrlKey) && e.key === 'n') {
        e.preventDefault();
        onNewConnection();
        return;
      }

      // Mod+Shift+L: Open library
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'l') {
        e.preventDefault();
        onOpenLibrary();
        return;
      }

      // Mod+Shift+F: Open full-text search
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        if (hasSession) {
          onFulltextSearch();
        }
        return;
      }

      // Mod+,: Settings
      if ((e.metaKey || e.ctrlKey) && e.key === ',') {
        e.preventDefault();
        onSettings();
        return;
      }

      // Escape: Close active tab or trigger escape handler
      if (e.key === 'Escape') {
        onEscape();
        return;
      }

      // Mod+W: Close active tab
      if ((e.metaKey || e.ctrlKey) && e.key === 'w') {
        e.preventDefault();
        if (hasActiveTab) {
          onCloseTab();
        }
        return;
      }

      // Mod+T: New query tab
      if ((e.metaKey || e.ctrlKey) && e.key === 't') {
        e.preventDefault();
        if (hasSession) {
          onNewQuery();
        }
        return;
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);
}
