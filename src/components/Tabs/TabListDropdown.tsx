// SPDX-License-Identifier: Apache-2.0

import { ChevronsUpDown, Pin } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Tooltip } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { getTabIcon } from './tabBarIcons';
import type { TabItem } from './tabBarTypes';

interface TabListDropdownProps {
  tabs: TabItem[];
  activeId?: string;
  onSelect?: (id: string) => void;
}

export function TabListDropdown({ tabs, activeId, onSelect }: TabListDropdownProps) {
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
