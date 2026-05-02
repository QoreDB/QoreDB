// SPDX-License-Identifier: Apache-2.0

import { Reorder } from 'framer-motion';
import { Plus } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Tooltip } from '@/components/ui/tooltip';
import { getModifierKey } from '@/utils/platform';
import { TabButton } from './TabButton';
import { TabContextMenu } from './TabContextMenu';
import { TabGroup } from './TabGroup';
import { TabListDropdown } from './TabListDropdown';
import { getGroupByConnection, subscribeGroupByConnection } from './tabBarPreferences';
import type { ConnectionLabelLookup, TabItem } from './tabBarTypes';

export type { TabItem } from './tabBarTypes';

interface TabBarProps {
  tabs?: TabItem[];
  activeId?: string;
  /** Lookup connection metadata for header rendering when grouping is on. */
  resolveConnection?: ConnectionLabelLookup;
  onSelect?: (id: string) => void;
  onClose?: (id: string) => void;
  onNew?: () => void;
  onReorder?: (tabs: TabItem[]) => void;
  onTogglePin?: (tabId: string) => void;
}

const UNGROUPED_KEY = '__ungrouped__';

export function TabBar({
  tabs = [],
  activeId,
  resolveConnection,
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
  const [groupByConnection, setGroupByConnectionState] = useState<boolean>(getGroupByConnection);
  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>({});
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => subscribeGroupByConnection(setGroupByConnectionState), []);

  const pinnedTabs = useMemo(() => tabs.filter(t => t.pinned), [tabs]);
  const unpinnedTabs = useMemo(() => tabs.filter(t => !t.pinned), [tabs]);

  const grouped = useMemo(() => {
    if (!groupByConnection) return null;
    const groups: { groupId: string; connectionId?: string; tabs: TabItem[] }[] = [];
    const indexByGroup = new Map<string, number>();
    for (const tab of unpinnedTabs) {
      const groupId = tab.connectionId ?? UNGROUPED_KEY;
      const idx = indexByGroup.get(groupId);
      if (idx === undefined) {
        indexByGroup.set(groupId, groups.length);
        groups.push({ groupId, connectionId: tab.connectionId, tabs: [tab] });
      } else {
        groups[idx].tabs.push(tab);
      }
    }
    return groups;
  }, [groupByConnection, unpinnedTabs]);

  const handleFlatReorder = useCallback(
    (reordered: TabItem[]) => {
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

  const handleGroupReorder = useCallback(
    (groupId: string, reordered: TabItem[]) => {
      if (!grouped) return;
      const merged: TabItem[] = [];
      for (const group of grouped) {
        if (group.groupId === groupId) {
          merged.push(...reordered);
        } else {
          merged.push(...group.tabs);
        }
      }
      onReorder?.([...pinnedTabs, ...merged]);
    },
    [grouped, pinnedTabs, onReorder]
  );

  const handleToggleCollapsed = useCallback((groupId: string) => {
    setCollapsedGroups(prev => ({ ...prev, [groupId]: !prev[groupId] }));
  }, []);

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

        {pinnedTabs.length > 0 && unpinnedTabs.length > 0 && (
          <div className="h-5 w-px bg-border/60 mx-0.5 shrink-0" />
        )}

        {grouped ? (
          <div className="flex items-center gap-1.5 flex-1 min-w-0">
            {grouped.map(group => {
              const label = group.connectionId
                ? resolveConnection?.(group.connectionId)
                : undefined;
              return (
                <TabGroup
                  key={group.groupId}
                  groupId={group.groupId}
                  label={label}
                  tabs={group.tabs}
                  activeId={activeId}
                  collapsed={!!collapsedGroups[group.groupId]}
                  onToggleCollapsed={handleToggleCollapsed}
                  onSelect={onSelect}
                  onClose={onClose}
                  onReorder={handleGroupReorder}
                  onContextMenu={handleContextMenu}
                />
              );
            })}
          </div>
        ) : (
          <Reorder.Group
            axis="x"
            values={unpinnedTabs}
            onReorder={handleFlatReorder}
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
        )}

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
