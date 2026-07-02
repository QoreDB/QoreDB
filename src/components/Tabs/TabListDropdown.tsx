// SPDX-License-Identifier: Apache-2.0

import { ChevronsUpDown, Pin } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
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

  return (
    <DropdownMenu>
      <Tooltip content={t('tabs.showAll')}>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            aria-label={t('tabs.showAll')}
            className="flex items-center justify-center w-7 h-7 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--q-accent)] data-[state=open]:bg-muted data-[state=open]:text-foreground"
          >
            <ChevronsUpDown size={14} />
          </button>
        </DropdownMenuTrigger>
      </Tooltip>
      <DropdownMenuContent align="end" className="min-w-52 max-h-72 text-xs">
        {tabs.map(tab => (
          <DropdownMenuItem
            key={tab.id}
            onSelect={() => onSelect?.(tab.id)}
            className={cn(
              'gap-2 py-1.5 text-xs',
              activeId === tab.id
                ? 'bg-muted font-medium text-foreground'
                : 'text-muted-foreground'
            )}
          >
            <span className="shrink-0 opacity-70">{getTabIcon(tab.type)}</span>
            <span className="truncate">{tab.title}</span>
            {tab.pinned && (
              <Pin className="ml-auto size-2.5 shrink-0 text-muted-foreground/50" />
            )}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
