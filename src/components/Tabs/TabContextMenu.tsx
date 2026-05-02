// SPDX-License-Identifier: Apache-2.0

import { Pin, PinOff, X } from 'lucide-react';
import { useCallback, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';

interface TabContextMenuProps {
  x: number;
  y: number;
  tabId: string;
  isPinned?: boolean;
  onClose: (tabId: string) => void;
  onTogglePin: (tabId: string) => void;
  onDismiss: () => void;
}

export function TabContextMenu({
  x,
  y,
  tabId,
  isPinned,
  onClose,
  onTogglePin,
  onDismiss,
}: TabContextMenuProps) {
  const { t } = useTranslation();
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const menu = menuRef.current;
    if (!menu) return;
    const firstItem = menu.querySelector<HTMLButtonElement>('[role="menuitem"]');
    firstItem?.focus();
  }, []);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const menu = menuRef.current;
      if (!menu) return;
      const items = Array.from(menu.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'));
      const currentIndex = items.indexOf(document.activeElement as HTMLButtonElement);

      switch (e.key) {
        case 'ArrowDown': {
          e.preventDefault();
          const next = currentIndex < items.length - 1 ? currentIndex + 1 : 0;
          items[next]?.focus();
          break;
        }
        case 'ArrowUp': {
          e.preventDefault();
          const prev = currentIndex > 0 ? currentIndex - 1 : items.length - 1;
          items[prev]?.focus();
          break;
        }
        case 'Escape':
          onDismiss();
          break;
        case 'Tab':
          e.preventDefault();
          onDismiss();
          break;
      }
    },
    [onDismiss]
  );

  return (
    <>
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: backdrop dismiss doesn't need keyboard */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: backdrop overlay */}
      <div
        className="fixed inset-0 z-50"
        onClick={onDismiss}
        onContextMenu={e => {
          e.preventDefault();
          onDismiss();
        }}
      />
      <div
        ref={menuRef}
        role="menu"
        aria-label={t('tabs.contextMenu')}
        className="fixed z-50 bg-popover border border-border rounded-md shadow-md py-1 min-w-36 text-xs"
        style={{ left: x, top: y }}
        onKeyDown={handleKeyDown}
      >
        <button
          type="button"
          role="menuitem"
          tabIndex={0}
          className="flex items-center gap-2 w-full px-3 py-1.5 text-left hover:bg-muted focus-visible:bg-muted focus-visible:outline-none transition-colors"
          onClick={() => onTogglePin(tabId)}
        >
          {isPinned ? <PinOff size={13} /> : <Pin size={13} />}
          {isPinned ? t('tabs.unpin') : t('tabs.pin')}
        </button>
        <hr className="h-px bg-border my-1 border-0" />
        <button
          type="button"
          role="menuitem"
          tabIndex={-1}
          className="flex items-center gap-2 w-full px-3 py-1.5 text-left hover:bg-muted focus-visible:bg-muted focus-visible:outline-none transition-colors text-destructive"
          onClick={() => onClose(tabId)}
        >
          <X size={13} />
          {t('tabs.close')}
        </button>
      </div>
    </>
  );
}
