// SPDX-License-Identifier: Apache-2.0

import { Reorder } from 'framer-motion';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Tooltip } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { TabButton } from './TabButton';
import type { ConnectionLabel, TabBarEnvironment, TabItem } from './tabBarTypes';

const ENVIRONMENT_DOT_CLASS: Record<TabBarEnvironment, string> = {
  development: 'bg-emerald-500',
  staging: 'bg-amber-500',
  production: 'bg-red-500',
};

interface TabGroupProps {
  /** Stable id used for keying and persistence (connectionId or 'ungrouped'). */
  groupId: string;
  /** Optional connection label. When omitted the group is rendered as "ungrouped". */
  label?: ConnectionLabel;
  tabs: TabItem[];
  activeId?: string;
  collapsed: boolean;
  onToggleCollapsed: (groupId: string) => void;
  onSelect?: (id: string) => void;
  onClose?: (id: string) => void;
  onReorder: (groupId: string, reordered: TabItem[]) => void;
  onContextMenu: (e: React.MouseEvent, tabId: string) => void;
}

export function TabGroup({
  groupId,
  label,
  tabs,
  activeId,
  collapsed,
  onToggleCollapsed,
  onSelect,
  onClose,
  onReorder,
  onContextMenu,
}: TabGroupProps) {
  const { t } = useTranslation();
  const groupHasActive = activeId !== undefined && tabs.some(tab => tab.id === activeId);

  const handleReorder = useCallback(
    (reordered: TabItem[]) => onReorder(groupId, reordered),
    [groupId, onReorder]
  );

  const headerName = label?.name ?? t('tabs.ungrouped');
  const tooltip = collapsed
    ? t('tabs.expand', { name: headerName })
    : t('tabs.collapse', { name: headerName });

  return (
    <div className="flex items-center gap-1 shrink-0 min-w-0">
      <Tooltip content={tooltip}>
        <button
          type="button"
          aria-expanded={!collapsed}
          aria-label={tooltip}
          className={cn(
            'flex items-center gap-1.5 h-7 px-2 rounded-md border border-border/60 bg-muted/40 text-xs text-muted-foreground hover:bg-muted/70 hover:text-foreground transition-colors shrink-0 max-w-50',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--q-accent)]',
            groupHasActive && 'border-border bg-muted/60 text-foreground'
          )}
          onClick={() => onToggleCollapsed(groupId)}
        >
          {collapsed ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
          {label?.environment && (
            <span
              className={cn(
                'w-1.5 h-1.5 rounded-full shrink-0',
                ENVIRONMENT_DOT_CLASS[label.environment]
              )}
              aria-hidden="true"
            />
          )}
          <span className="truncate font-medium text-foreground/80">{headerName}</span>
          <span className="ml-1 tabular-nums text-[10px] text-muted-foreground/80">
            {tabs.length}
          </span>
        </button>
      </Tooltip>
      {!collapsed && (
        <Reorder.Group
          axis="x"
          values={tabs}
          onReorder={handleReorder}
          className="flex items-center gap-0 min-w-0"
          as="div"
        >
          {tabs.map(tab => (
            <TabButton
              key={tab.id}
              tab={tab}
              isActive={activeId === tab.id}
              onSelect={onSelect}
              onClose={onClose}
              onContextMenu={onContextMenu}
            />
          ))}
        </Reorder.Group>
      )}
    </div>
  );
}
