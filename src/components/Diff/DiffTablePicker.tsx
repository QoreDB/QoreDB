// SPDX-License-Identifier: BUSL-1.1

import { ChevronDown, Loader2, Search, Table2, X } from 'lucide-react';
/**
 * DiffTablePicker - Searchable dropdown for selecting tables
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { type Collection, listCollections, type Namespace } from '@/lib/tauri';
import { cn } from '@/lib/utils';

interface DiffTablePickerProps {
  sessionId: string;
  namespace: Namespace;
  value?: string;
  onSelect: (tableName: string) => void;
  disabled?: boolean;
  placeholder?: string;
}

export function DiffTablePicker({
  sessionId,
  namespace,
  value,
  onSelect,
  disabled = false,
  placeholder,
}: DiffTablePickerProps) {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [tables, setTables] = useState<Collection[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Load tables when opened
  useEffect(() => {
    if (!isOpen || !sessionId) return;

    setLoading(true);
    listCollections(sessionId, namespace, search || undefined, 1, 100)
      .then(res => {
        if (res.success && res.data) {
          setTables(res.data.collections);
        }
      })
      .catch(console.error)
      .finally(() => setLoading(false));
  }, [isOpen, sessionId, namespace, search]);

  // Reset selected index when search changes
  useEffect(() => {
    setSelectedIndex(0);
  }, []);

  // Focus input when opened
  useEffect(() => {
    if (isOpen) {
      inputRef.current?.focus();
    }
  }, [isOpen]);

  // Close on click outside
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!isOpen) return;

      if (e.key === 'Escape') {
        e.preventDefault();
        setIsOpen(false);
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        setSelectedIndex(i => Math.min(i + 1, tables.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSelectedIndex(i => Math.max(i - 1, 0));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        if (tables[selectedIndex]) {
          onSelect(tables[selectedIndex].name);
          setIsOpen(false);
          setSearch('');
        }
      }
    },
    [isOpen, tables, selectedIndex, onSelect]
  );

  const handleSelect = (tableName: string) => {
    onSelect(tableName);
    setIsOpen(false);
    setSearch('');
  };

  const handleClear = (e: React.MouseEvent) => {
    e.stopPropagation();
    onSelect('');
  };

  return (
    <div ref={containerRef} className="relative w-full" onKeyDown={handleKeyDown}>
      <Button
        variant="outline"
        className={cn('w-full justify-between font-normal', !value && 'text-muted-foreground')}
        onClick={() => !disabled && setIsOpen(!isOpen)}
        disabled={disabled}
      >
        <span className="flex items-center gap-2 truncate">
          {value ? (
            <>
              <Table2 size={14} className="shrink-0 text-muted-foreground" />
              <span className="truncate">{value}</span>
            </>
          ) : (
            (placeholder ?? t('diff.selectTable'))
          )}
        </span>
        <span className="flex items-center gap-1">
          {value && (
            <X
              size={14}
              className="text-muted-foreground hover:text-foreground cursor-pointer"
              onClick={handleClear}
            />
          )}
          <ChevronDown size={14} className="text-muted-foreground" />
        </span>
      </Button>

      {isOpen && (
        <div className="absolute z-50 mt-1 w-full bg-popover border border-border rounded-md shadow-lg overflow-hidden">
          {/* Search input */}
          <div className="flex items-center px-3 py-2 border-b border-border">
            <Search size={14} className="text-muted-foreground mr-2" />
            <input
              ref={inputRef}
              type="text"
              className="flex-1 bg-transparent outline-none text-sm placeholder:text-muted-foreground"
              placeholder={t('dbtree.searchPlaceholder')}
              value={search}
              onChange={e => setSearch(e.target.value)}
            />
            {loading && <Loader2 size={14} className="animate-spin text-muted-foreground" />}
          </div>

          {/* Results list */}
          <div className="max-h-60 overflow-y-auto py-1">
            {tables.length === 0 && !loading ? (
              <div className="px-3 py-4 text-sm text-muted-foreground text-center">
                {t('common.noResults')}
              </div>
            ) : (
              tables.map((table, i) => (
                <button
                  key={table.name}
                  type="button"
                  className={cn(
                    'w-full flex items-center gap-2 px-3 py-2 text-sm text-left transition-colors',
                    i === selectedIndex ? 'bg-accent text-accent-foreground' : 'hover:bg-muted/50'
                  )}
                  onClick={() => handleSelect(table.name)}
                  onMouseEnter={() => setSelectedIndex(i)}
                >
                  <Table2 size={14} className="shrink-0 text-muted-foreground" />
                  <span className="truncate">{table.name}</span>
                  <span className="ml-auto text-xs text-muted-foreground">
                    {table.collection_type}
                  </span>
                </button>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}
