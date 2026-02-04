import { useEffect } from 'react';

interface WebviewGuardsOptions {
  allowContextMenuSelector?: string;
  allowShortcutSelector?: string;
  disableContextMenu?: boolean;
  disableShortcuts?: boolean;
}

function isAllowedTarget(target: EventTarget | null, selector?: string): boolean {
  if (!selector) return false;
  if (!(target instanceof HTMLElement)) return false;
  return Boolean(target.closest(selector));
}

export function useWebviewGuards({
  allowContextMenuSelector = '[data-allow-context-menu],[data-slot="context-menu-trigger"]',
  allowShortcutSelector = '[data-allow-webview-shortcuts]',
  disableContextMenu = true,
  disableShortcuts = true,
}: WebviewGuardsOptions = {}) {
  useEffect(() => {
    if (!disableContextMenu) return;

    const handleContextMenu = (event: MouseEvent) => {
      if (isAllowedTarget(event.target, allowContextMenuSelector)) return;
      event.preventDefault();
    };

    window.addEventListener('contextmenu', handleContextMenu, { capture: true });
    return () => window.removeEventListener('contextmenu', handleContextMenu, { capture: true });
  }, [allowContextMenuSelector, disableContextMenu]);

  useEffect(() => {
    if (!disableShortcuts) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      if (isAllowedTarget(event.target, allowShortcutSelector)) return;

      const key = event.key.toLowerCase();
      const isMod = event.ctrlKey || event.metaKey;

      if (!isMod) return;

      const isPrint = key === 'p' && !event.shiftKey && !event.altKey;
      const isReload = key === 'r' && !event.altKey;
      const isViewSource = key === 'u' && !event.shiftKey && !event.altKey;
      const isDevtools =
        (key === 'i' && event.shiftKey) ||
        (key === 'j' && event.shiftKey) ||
        (key === 'c' && event.shiftKey);

      if (isPrint || isReload || isViewSource || isDevtools) {
        event.preventDefault();
      }
    };

    window.addEventListener('keydown', handleKeyDown, { capture: true });
    return () => window.removeEventListener('keydown', handleKeyDown, { capture: true });
  }, [allowShortcutSelector, disableShortcuts]);
}
