// SPDX-License-Identifier: Apache-2.0

import { Command, Database, FileCode, Folder, Search, Star } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { cn } from '@/lib/utils';
import { getFavorites, type HistoryEntry, searchHistory } from '../../lib/history';
import {
  listFolders,
  listItems,
  type QueryFolder,
  type QueryLibraryItem,
} from '../../lib/queryLibrary';
import { listSavedConnections, type SavedConnection } from '../../lib/tauri';

interface GlobalSearchProps {
  isOpen: boolean;
  onClose: () => void;
  onSelect?: (result: SearchResult) => void;
  commands?: CommandItem[];
}

export interface CommandItem {
  id: string;
  label: string;
  sublabel?: string;
  shortcut?: string;
}

export interface SearchResult {
  type: 'command' | 'connection' | 'query' | 'favorite' | 'library';
  id: string;
  label: string;
  sublabel?: string;
  shortcut?: string;
  data?: SavedConnection | HistoryEntry | QueryLibraryItem;
}

const DEFAULT_PROJECT = 'default';

export function GlobalSearch({ isOpen, onClose, onSelect, commands = [] }: GlobalSearchProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchResult[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [connections, setConnections] = useState<SavedConnection[]>([]);
  const [libraryItems, setLibraryItems] = useState<QueryLibraryItem[]>([]);
  const [libraryFolders, setLibraryFolders] = useState<QueryFolder[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isOpen) {
      inputRef.current?.focus();
      setQuery('');
      setResults(
        commands.map(cmd => ({
          type: 'command',
          id: cmd.id,
          label: cmd.label,
          sublabel: cmd.sublabel,
          shortcut: cmd.shortcut,
        }))
      );
      setSelectedIndex(0);

      listSavedConnections(DEFAULT_PROJECT).then(setConnections).catch(console.error);

      try {
        setLibraryItems(listItems());
        setLibraryFolders(listFolders());
      } catch (err) {
        console.error(err);
      }
    }
  }, [isOpen, commands.map]);

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (!isOpen) return;

      if (e.key === 'Escape') {
        onClose();
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        setSelectedIndex(i => Math.min(i + 1, results.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSelectedIndex(i => Math.max(i - 1, 0));
      } else if (e.key === 'Enter' && results[selectedIndex]) {
        onSelect?.(results[selectedIndex]);
        onClose();
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, results, selectedIndex, onClose, onSelect]);

  const handleSearch = useCallback(
    (value: string) => {
      setQuery(value);
      setSelectedIndex(0);

      const trimmed = value.trim();
      const lowerQuery = trimmed.toLowerCase();
      const searchResults: SearchResult[] = [];

      // Commands first (always available, also searchable)
      const matchingCommands = trimmed
        ? commands.filter(cmd => cmd.label.toLowerCase().includes(lowerQuery))
        : commands;
      matchingCommands.forEach(cmd => {
        searchResults.push({
          type: 'command',
          id: cmd.id,
          label: cmd.label,
          sublabel: cmd.sublabel,
          shortcut: cmd.shortcut,
        });
      });

      if (!trimmed) {
        setResults(searchResults);
        return;
      }

      const folderById = new Map<string, string>();
      libraryFolders.forEach(f => folderById.set(f.id, f.name));

      // Search saved connections
      connections.forEach(conn => {
        const matches =
          conn.name.toLowerCase().includes(lowerQuery) ||
          conn.host.toLowerCase().includes(lowerQuery) ||
          (conn.database?.toLowerCase().includes(lowerQuery) ?? false);

        if (matches) {
          searchResults.push({
            type: 'connection',
            id: conn.id,
            label: conn.name,
            sublabel: `${conn.driver} · ${conn.host}`,
            data: conn,
          });
        }
      });

      // Search favorites (higher priority than history)
      const favorites = getFavorites();
      favorites.forEach(fav => {
        if (fav.query.toLowerCase().includes(lowerQuery)) {
          searchResults.push({
            type: 'favorite',
            id: `fav-${fav.id}`,
            label: fav.query.substring(0, 60) + (fav.query.length > 60 ? '...' : ''),
            sublabel: fav.database ?? fav.driver,
            data: fav,
          });
        }
      });

      // Search history
      const historyResults = searchHistory(lowerQuery);
      historyResults.slice(0, 5).forEach(entry => {
        if (favorites.some(f => f.id === entry.id)) return;

        searchResults.push({
          type: 'query',
          id: `hist-${entry.id}`,
          label: entry.query.substring(0, 60) + (entry.query.length > 60 ? '...' : ''),
          sublabel: entry.database ?? entry.driver,
          data: entry,
        });
      });

      // Search query library
      libraryItems.forEach(item => {
        const matches =
          item.title.toLowerCase().includes(lowerQuery) ||
          item.query.toLowerCase().includes(lowerQuery) ||
          item.tags.some(tag => tag.includes(lowerQuery));

        if (!matches) return;

        const folderName = item.folderId ? folderById.get(item.folderId) : undefined;
        const sublabelParts = [];
        sublabelParts.push(t('library.searchLabel'));
        if (folderName) sublabelParts.push(folderName);
        if (item.tags.length)
          sublabelParts.push(
            item.tags
              .slice(0, 2)
              .map(tag => `#${tag}`)
              .join(' ')
          );

        searchResults.push({
          type: 'library',
          id: `lib-${item.id}`,
          label: item.title,
          sublabel: sublabelParts.join(' · '),
          data: item,
        });
      });

      setResults(searchResults.slice(0, 10));
    },
    [commands, connections, libraryFolders, libraryItems, t]
  );

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[20vh] bg-background/80 backdrop-blur-sm p-4"
      onClick={onClose}
    >
      <div
        className="w-full max-w-lg bg-background border border-border rounded-lg shadow-2xl overflow-hidden flex flex-col ring-1 ring-border"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center px-4 border-b border-border">
          <Search className="w-5 h-5 text-muted-foreground mr-2" />
          <input
            ref={inputRef}
            className="flex-1 h-14 bg-transparent outline-none placeholder:text-muted-foreground text-base"
            type="text"
            placeholder={t('search.placeholder')}
            value={query}
            onChange={e => handleSearch(e.target.value)}
          />
        </div>

        {results.length > 0 ? (
          <div className="max-h-75 overflow-y-auto py-1">
            {results.map((result, i) => (
              <button
                key={result.id}
                className={cn(
                  'w-full flex items-center gap-3 px-4 py-2.5 text-sm cursor-pointer transition-colors text-left',
                  i === selectedIndex
                    ? 'bg-accent text-accent-foreground'
                    : 'text-foreground hover:bg-muted/50'
                )}
                onClick={() => {
                  onSelect?.(result);
                  onClose();
                }}
                onMouseEnter={() => setSelectedIndex(i)}
              >
                <span
                  className={cn(
                    'flex items-center justify-center text-muted-foreground',
                    i === selectedIndex && 'text-accent-foreground/70'
                  )}
                >
                  {result.type === 'command' ? (
                    <Command size={16} />
                  ) : result.type === 'connection' ? (
                    <Database size={16} />
                  ) : result.type === 'favorite' ? (
                    <Star size={16} />
                  ) : result.type === 'library' ? (
                    (result.data as QueryLibraryItem | undefined)?.isFavorite ? (
                      <Star size={16} />
                    ) : (
                      <Folder size={16} />
                    )
                  ) : (
                    <FileCode size={16} />
                  )}
                </span>

                <div className="flex flex-col flex-1 overflow-hidden">
                  <span className="font-medium truncate">{result.label}</span>
                  {result.sublabel && (
                    <span
                      className={cn(
                        'text-xs truncate opacity-70',
                        i !== selectedIndex && 'text-muted-foreground'
                      )}
                    >
                      {result.sublabel}
                    </span>
                  )}
                </div>

                {result.shortcut && (
                  <kbd
                    className={cn(
                      'ml-auto pointer-events-none inline-flex h-5 select-none items-center gap-1 rounded border bg-muted px-1.5 font-mono text-[10px] font-medium text-muted-foreground opacity-100',
                      i === selectedIndex &&
                        'border-accent-foreground/30 bg-accent-foreground/10 text-accent-foreground'
                    )}
                  >
                    {result.shortcut}
                  </kbd>
                )}
              </button>
            ))}
          </div>
        ) : (
          <div className="px-4 py-8 text-center text-sm text-muted-foreground">
            {query.trim() ? t('search.noResults') : t('browser.typeToSearch')}
          </div>
        )}

        <div className="flex items-center justify-end gap-3 px-4 py-2 border-t border-border bg-muted/20 text-xs text-muted-foreground select-none">
          <div className="flex items-center gap-1">
            <kbd className="px-1.5 py-0.5 rounded bg-muted border border-border font-mono text-[10px]">
              ↑↓
            </kbd>{' '}
            {t('browser.navigate')}
          </div>
          <div className="flex items-center gap-1">
            <kbd className="px-1.5 py-0.5 rounded bg-muted border border-border font-mono text-[10px]">
              ↵
            </kbd>{' '}
            {t('browser.select')}
          </div>
          <div className="flex items-center gap-1">
            <kbd className="px-1.5 py-0.5 rounded bg-muted border border-border font-mono text-[10px]">
              esc
            </kbd>{' '}
            {t('browser.close')}
          </div>
        </div>
      </div>
    </div>
  );
}
