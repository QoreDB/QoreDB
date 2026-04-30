// SPDX-License-Identifier: Apache-2.0

import { Reorder } from 'framer-motion';
import { X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Tooltip } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { getTabIcon, isTemporaryTab } from './tabBarIcons';
import type { TabItem } from './tabBarTypes';

interface TabButtonProps {
  tab: TabItem;
  isActive: boolean;
  onSelect?: (id: string) => void;
  onClose?: (id: string) => void;
  onContextMenu: (e: React.MouseEvent, tabId: string) => void;
}

export function TabButton({ tab, isActive, onSelect, onClose, onContextMenu }: TabButtonProps) {
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
