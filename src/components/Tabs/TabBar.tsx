// SPDX-License-Identifier: Apache-2.0

import { cn } from '@/lib/utils';
import { getModifierKey } from '@/utils/platform';
import { Reorder } from 'framer-motion';
import {
  BookOpen,
  Camera,
  Database,
  FileCode,
  GitCompare,
  Network,
  Pin,
  PinOff,
  Plus,
  Settings,
  Table,
  X,
} from 'lucide-react';
import { useCallback, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

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
    | 'notebook';
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

        <button
          type="button"
          className="flex items-center justify-center w-8 h-8 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground transition-colors shrink-0"
          onClick={onNew}
          title={t('tabs.newQuery', { modifier: getModifierKey() })}
        >
          <Plus size={16} />
        </button>
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
        className={cn(
          'flex items-center gap-2 py-1.5 h-8.5 text-xs rounded-t-md border-t border-x border-transparent transition-all',
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
        <button
          type="button"
          className={cn(
            'absolute right-2 top-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 p-0.5 rounded-sm hover:bg-muted-foreground/20 text-muted-foreground transition-all shrink-0',
            'cursor-pointer'
          )}
          onClick={() => onClose?.(tab.id)}
          aria-label="Close tab"
        >
          <X size={12} />
        </button>
      )}
    </Reorder.Item>
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
        className="fixed z-50 bg-popover border border-border rounded-md shadow-md py-1 min-w-36 text-xs"
        style={{ left: x, top: y }}
      >
        <button
          type="button"
          className="flex items-center gap-2 w-full px-3 py-1.5 text-left hover:bg-muted transition-colors"
          onClick={() => onTogglePin(tabId)}
        >
          {isPinned ? <PinOff size={13} /> : <Pin size={13} />}
          {isPinned ? t('tabs.unpin') : t('tabs.pin')}
        </button>
        <div className="h-px bg-border my-1" />
        <button
          type="button"
          className="flex items-center gap-2 w-full px-3 py-1.5 text-left hover:bg-muted transition-colors text-destructive"
          onClick={() => onClose(tabId)}
        >
          <X size={13} />
          {t('tabs.close')}
        </button>
      </div>
    </>
  );
}
