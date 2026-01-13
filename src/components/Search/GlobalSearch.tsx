import { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Search, Database, Table, FileCode } from 'lucide-react';
import { cn } from '@/lib/utils';

interface GlobalSearchProps {
  isOpen: boolean;
  onClose: () => void;
  onSelect?: (result: SearchResult) => void;
}

interface SearchResult {
  type: 'connection' | 'table' | 'query';
  id: string;
  label: string;
  sublabel?: string;
}

export function GlobalSearch({ isOpen, onClose, onSelect }: GlobalSearchProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchResult[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isOpen) {
      inputRef.current?.focus();
      setQuery('');
      setResults([]);
      setSelectedIndex(0);
    }
  }, [isOpen]);

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

  // TODO: Implement actual search
  function handleSearch(value: string) {
    setQuery(value);
    setSelectedIndex(0);
    
    // Placeholder results
    if (value.trim()) {
      setResults([
        { type: 'connection', id: '1', label: 'Production DB', sublabel: 'postgres' },
        { type: 'table', id: '2', label: 'users', sublabel: 'public.users' },
      ]);
    } else {
      setResults([]);
    }
  }

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
            placeholder={t('browser.searchPlaceholder')}
            value={query}
            onChange={e => handleSearch(e.target.value)}
          />
        </div>
        
        {results.length > 0 && (
          <div className="max-h-[300px] overflow-y-auto py-1">
            {results.map((result, i) => (
              <button
                key={result.id}
                className={cn(
                  "w-full flex items-center gap-3 px-4 py-2.5 text-sm cursor-pointer transition-colors text-left",
                  i === selectedIndex 
                    ? "bg-accent text-accent-foreground" 
                    : "text-foreground hover:bg-muted/50"
                )}
                onClick={() => {
                  onSelect?.(result);
                  onClose();
                }}
                onMouseEnter={() => setSelectedIndex(i)}
              >
                <span className={cn(
                  "flex items-center justify-center text-muted-foreground",
                  i === selectedIndex && "text-accent-foreground/70"
                )}>
                  {result.type === 'connection' ? <Database size={16} /> : 
                   result.type === 'table' ? <Table size={16} /> : <FileCode size={16} />}
                </span>
                
                <div className="flex flex-col flex-1 overflow-hidden">
                    <span className="font-medium truncate">{result.label}</span>
                    {result.sublabel && (
                      <span className={cn(
                        "text-xs truncate opacity-70",
                        i !== selectedIndex && "text-muted-foreground"
                      )}>
                        {result.sublabel}
                      </span>
                    )}
                </div>
              </button>
            ))}
          </div>
        )}
        
        {query === '' && results.length === 0 && (
            <div className="px-4 py-8 text-center text-sm text-muted-foreground">
                {t('browser.typeToSearch')}
            </div>
        )}
        
        <div className="flex items-center justify-end gap-3 px-4 py-2 border-t border-border bg-muted/20 text-xs text-muted-foreground select-none">
          <div className="flex items-center gap-1"><kbd className="px-1.5 py-0.5 rounded bg-muted border border-border font-mono text-[10px]">↑↓</kbd> {t('browser.navigate')}</div>
          <div className="flex items-center gap-1"><kbd className="px-1.5 py-0.5 rounded bg-muted border border-border font-mono text-[10px]">↵</kbd> {t('browser.select')}</div>
          <div className="flex items-center gap-1"><kbd className="px-1.5 py-0.5 rounded bg-muted border border-border font-mono text-[10px]">esc</kbd> {t('browser.close')}</div>
        </div>
      </div>
    </div>
  );
}
