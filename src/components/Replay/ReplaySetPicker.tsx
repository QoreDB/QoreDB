// SPDX-License-Identifier: BUSL-1.1

import { ChevronsUpDown, ListRestart, Search, Trash2 } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import type { ReplaySetSummary } from '@/lib/replay';
import { cn } from '@/lib/utils';

interface ReplaySetPickerProps {
  sets: ReplaySetSummary[];
  activeSlug: string | null;
  activeName?: string;
  loading: boolean;
  onSelect: (slug: string) => void;
  onDelete: (slug: string) => void;
}

export function ReplaySetPicker({
  sets,
  activeSlug,
  activeName,
  loading,
  onSelect,
  onDelete,
}: ReplaySetPickerProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');

  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase();
    if (!needle) return sets;
    return sets.filter(set => set.name.toLowerCase().includes(needle));
  }, [search, sets]);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="sm" className="h-7 max-w-64 gap-1.5 px-2">
          <ListRestart size={14} className="shrink-0 text-accent" />
          <span className="truncate text-sm font-medium">
            {activeName ?? t('replay.selectSet')}
          </span>
          <ChevronsUpDown size={12} className="shrink-0 text-muted-foreground" />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-72 p-1">
        <div className="relative mb-1">
          <Search
            size={12}
            className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground"
          />
          <Input
            autoFocus
            value={search}
            onChange={event => setSearch(event.target.value)}
            placeholder={t('replay.searchSets')}
            className="h-7 pl-6 text-xs"
          />
        </div>

        {loading ? (
          <p className="px-2 py-6 text-center text-xs text-muted-foreground">
            {t('common.loading')}
          </p>
        ) : filtered.length === 0 ? (
          <p className="px-2 py-6 text-center text-xs text-muted-foreground">
            {sets.length === 0 ? t('replay.noSets') : t('replay.noMatchingSet')}
          </p>
        ) : (
          <ul className="max-h-72 overflow-y-auto">
            {filtered.map(set => (
              <li
                key={set.slug}
                className={cn(
                  'group flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-[var(--color-bg-2)]',
                  activeSlug === set.slug && 'bg-[var(--color-bg-2)]'
                )}
              >
                <button
                  type="button"
                  className="min-w-0 flex-1 text-left focus:outline-none focus-visible:ring-1 focus-visible:ring-ring rounded-sm"
                  onClick={() => {
                    onSelect(set.slug);
                    setOpen(false);
                  }}
                >
                  <p className="text-xs font-medium truncate">{set.name}</p>
                  <p className="text-[11px] text-muted-foreground truncate">
                    {t('replay.entriesTotal', { count: set.entry_count })} · {set.environment}
                  </p>
                </button>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-5 w-5 shrink-0 p-0 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                  onClick={() => onDelete(set.slug)}
                  aria-label={t('replay.deleteSet')}
                >
                  <Trash2 size={12} />
                </Button>
              </li>
            ))}
          </ul>
        )}
      </PopoverContent>
    </Popover>
  );
}
