// SPDX-License-Identifier: Apache-2.0

import { Reorder } from 'framer-motion';
import {
  BookOpen,
  Camera,
  ChevronsUpDown,
  Database,
  FileCode,
  GitCompare,
  History,
  Network,
  Pin,
  PinOff,
  Plus,
  Settings,
  Table,
  X,
} from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Tooltip } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { getModifierKey } from '@/utils/platform';

export interface TabItem {
  id: string;
  title: string;
  pinned?: boolean;
  type:
    | 'query'
    | 'table'
    | 'database'
    | 'settings'
    | 'diff'
    | 'federation'
    | 'snapshots'
    | 'notebook'
    | 'time-travel';
}

interface TabBarProps {
  tabs?: TabItem[];
  activeId?: string;
  onSelect?: (id: string) => void;
  onClose?: (id: string) => void;
  onNew?: () => void;
  onReorder?: (tabs: TabItem[]) => void;
  onTogglePin?: (tabId: string) => void;
}

function getTabIcon(type: TabItem['type']) {
  switch (type) {
    case 'query':
      return <FileCode size={14} />;
    case 'table':
      return <Table size={14} />;
    case 'database':
      return <Database size={14} />;
    case 'settings':
      return <Settings size={14} />;
    case 'diff':
      return <GitCompare size={14} />;
    case 'federation':
      return <Network size={14} className="text-accent" />;
    case 'snapshots':
      return <Camera size={14} />;
    case 'notebook':
      return <BookOpen size={14} />;
    case 'time-travel':
      return <History size={14} />;
  }
}

const isTemporaryTab = (type: TabItem['type']) => type === 'query';

export function TabBar({
  tabs = [],
  activeId,
  onSelect,
  onClose,
  onNew,
  onReorder,
  onTogglePin,
}: TabBarProps) {
  const { t } = useTranslation();
  const [contextMenu, setContextMenu] = useState<{ tabId: string; x: number; y: number } | null>(
    null
  );
  const containerRef = useRef<HTMLDivElement>(null);

  const pinnedTabs = tabs.filter(t => t.pinned);
  const unpinnedTabs = tabs.filter(t => !t.pinned);

  const handleReorder = useCallback(
    (reordered: TabItem[]) => {
      // Keep pinned tabs in their group, only reorder within unpinned
      onReorder?.([...pinnedTabs, ...reordered]);
    },
    [pinnedTabs, onReorder]
  );

  const handlePinnedReorder = useCallback(
    (reordered: TabItem[]) => {
      onReorder?.([...reordered, ...unpinnedTabs]);
    },
    [unpinnedTabs, onReorder]
  );

  const handleContextMenu = useCallback((e: React.MouseEvent, tabId: string) => {
    e.preventDefault();
    setContextMenu({ tabId, x: e.clientX, y: e.clientY });
  }, []);

  const closeContextMenu = useCallback(() => setContextMenu(null), []);

  return (
    <>
      <div
        ref={containerRef}
        className="flex items-center w-full bg-background border-b border-border h-10 select-none pl-1 gap-0 overflow-x-auto overflow-y-hidden no-scrollbar"
        role="tablist"
        aria-label={t('a11y.tabBar')}
        onClick={closeContextMenu}
        onKeyDown={e => {
          if (e.key === 'Escape') closeContextMenu();
        }}
      >
        {/* Pinned tabs */}
        {pinnedTabs.length > 0 && (
          <Reorder.Group
            axis="x"
            values={pinnedTabs}
            onReorder={handlePinnedReorder}
            className="flex items-center gap-0"
            as="div"
          >
            {pinnedTabs.map(tab => (
              <TabButton
                key={tab.id}
                tab={tab}
                isActive={activeId === tab.id}
                onSelect={onSelect}
                onClose={onClose}
                onContextMenu={handleContextMenu}
              />
            ))}
          </Reorder.Group>
        )}

        {/* Separator between pinned and unpinned */}
        {pinnedTabs.length > 0 && unpinnedTabs.length > 0 && (
          <div className="h-5 w-px bg-border/60 mx-0.5 shrink-0" />
        )}

        {/* Unpinned tabs */}
        <Reorder.Group
          axis="x"
          values={unpinnedTabs}
          onReorder={handleReorder}
          className="flex items-center gap-0 flex-1 min-w-0"
          as="div"
        >
          {unpinnedTabs.map(tab => (
            <TabButton
              key={tab.id}
              tab={tab}
              isActive={activeId === tab.id}
              onSelect={onSelect}
              onClose={onClose}
              onContextMenu={handleContextMenu}
            />
          ))}
        </Reorder.Group>

        {/* Tab list dropdown — shows all tabs for quick navigation */}
        {tabs.length > 5 && <TabListDropdown tabs={tabs} activeId={activeId} onSelect={onSelect} />}

        <Tooltip content={t('tabs.newQuery', { modifier: getModifierKey() })}>
          <button
            type="button"
            className="flex items-center justify-center w-8 h-8 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground transition-colors shrink-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--q-accent)]"
            onClick={onNew}
          >
            <Plus size={16} />
          </button>
        </Tooltip>
      </div>

      {/* Context menu */}
      {contextMenu && (
        <TabContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          tabId={contextMenu.tabId}
          isPinned={tabs.find(t => t.id === contextMenu.tabId)?.pinned}
          onClose={id => {
            onClose?.(id);
            closeContextMenu();
          }}
          onTogglePin={id => {
            onTogglePin?.(id);
            closeContextMenu();
          }}
          onDismiss={closeContextMenu}
        />
      )}
    </>
  );
}

// --- Individual tab button wrapped in Reorder.Item ---

interface TabButtonProps {
  tab: TabItem;
  isActive: boolean;
  onSelect?: (id: string) => void;
  onClose?: (id: string) => void;
  onContextMenu: (e: React.MouseEvent, tabId: string) => void;
}

function TabButton({ tab, isActive, onSelect, onClose, onContextMenu }: TabButtonProps) {
  const { t } = useTranslation();
  return (
    <Reorder.Item
      value={tab}
      layout="position"
      transition={{ duration: 0.15 }}
      className="group relative mt-1.25"
      style={{ cursor: 'grab' }}
      whileDrag={{ cursor: 'grabbing', scale: 1.02, zIndex: 50 }}
    >
      <button
        type="button"
        role="tab"
        aria-selected={isActive}
        aria-controls={`tabpanel-${tab.id}`}
        id={`tab-${tab.id}`}
        className={cn(
          'flex items-center gap-2 py-1.5 h-8.5 text-xs rounded-t-md border-t border-x border-transparent transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--q-accent)] focus-visible:ring-inset',
          tab.pinned ? 'pl-2 pr-2 min-w-9 max-w-9' : 'pl-3 pr-8 min-w-35 max-w-50',
          isActive
            ? 'bg-background text-foreground font-medium border-border -mb-px shadow-sm z-10'
            : 'text-muted-foreground hover:bg-muted/50 hover:text-foreground'
        )}
        onClick={() => onSelect?.(tab.id)}
        onMouseDown={e => {
          if (e.button === 1) {
            e.preventDefault();
            onClose?.(tab.id);
          }
        }}
        onContextMenu={e => onContextMenu(e, tab.id)}
        title={tab.title}
      >
        <span
          className={cn('shrink-0', isTemporaryTab(tab.type) ? 'text-accent/70' : 'opacity-70')}
        >
          {getTabIcon(tab.type)}
        </span>
        {!tab.pinned && (
          <span className={cn('truncate flex-1 text-left', isTemporaryTab(tab.type) && 'italic')}>
            {tab.title}
          </span>
        )}
      </button>
      {/* Close button — hidden for pinned tabs */}
      {!tab.pinned && (
        <Tooltip content={t('tabs.close')}>
          <button
            type="button"
            className={cn(
              'absolute right-2 top-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 p-0.5 rounded-sm hover:bg-muted-foreground/20 text-muted-foreground transition-all shrink-0',
              'cursor-pointer'
            )}
            onClick={() => onClose?.(tab.id)}
            aria-label={t('tabs.close')}
          >
            <X size={12} />
          </button>
        </Tooltip>
      )}
    </Reorder.Item>
  );
}

// --- Tab list dropdown (overflow navigation) ---

function TabListDropdown({
  tabs,
  activeId,
  onSelect,
}: {
  tabs: TabItem[];
  activeId?: string;
  onSelect?: (id: string) => void;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  return (
    <div className="relative shrink-0">
      <Tooltip content={t('tabs.showAll')}>
        <button
          type="button"
          className="flex items-center justify-center w-7 h-7 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--q-accent)]"
          onClick={() => setOpen(!open)}
        >
          <ChevronsUpDown size={14} />
        </button>
      </Tooltip>
      {open && (
        <>
          {/* biome-ignore lint/a11y/useKeyWithClickEvents: backdrop dismiss */}
          {/* biome-ignore lint/a11y/noStaticElementInteractions: backdrop overlay */}
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div
            role="listbox"
            aria-label={t('tabs.showAll')}
            className="absolute right-0 top-full mt-1 z-50 bg-popover border border-border rounded-md shadow-lg py-1 min-w-52 max-h-64 overflow-y-auto text-xs"
          >
            {tabs.map(tab => (
              <button
                key={tab.id}
                type="button"
                role="option"
                aria-selected={activeId === tab.id}
                className={cn(
                  'flex items-center gap-2 w-full px-3 py-1.5 text-left hover:bg-muted transition-colors',
                  activeId === tab.id && 'bg-muted font-medium text-foreground',
                  activeId !== tab.id && 'text-muted-foreground'
                )}
                onClick={() => {
                  onSelect?.(tab.id);
                  setOpen(false);
                }}
              >
                <span className="shrink-0 opacity-70">{getTabIcon(tab.type)}</span>
                <span className="truncate">{tab.title}</span>
                {tab.pinned && <Pin size={10} className="ml-auto text-muted-foreground/50" />}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

// --- Context menu ---

interface TabContextMenuProps {
  x: number;
  y: number;
  tabId: string;
  isPinned?: boolean;
  onClose: (tabId: string) => void;
  onTogglePin: (tabId: string) => void;
  onDismiss: () => void;
}

function TabContextMenu({
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

  // Auto-focus first item and handle keyboard navigation
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
      {/* Backdrop to close menu */}
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
