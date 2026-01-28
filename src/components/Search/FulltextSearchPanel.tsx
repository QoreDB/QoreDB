import { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Search, Database, Table2, Loader2, X, ChevronRight, AlertCircle } from 'lucide-react';
import { cn } from '@/lib/utils';
import { fulltextSearch, FulltextMatch, FulltextSearchResponse, Namespace, Value, SearchFilter } from '../../lib/tauri';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Switch } from '../ui/switch';
import { Label } from '../ui/label';

interface FulltextSearchPanelProps {
  isOpen: boolean;
  onClose: () => void;
  sessionId: string | null;
  onNavigateToTable?: (namespace: Namespace, tableName: string, searchFilter?: SearchFilter) => void;
}

interface GroupedMatches {
  namespace: Namespace;
  tableName: string;
  matches: FulltextMatch[];
}

function formatValue(value: Value): string {
  if (value === null) return 'NULL';
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  if (typeof value === 'number') return String(value);
  if (typeof value === 'string') return value;
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

function groupMatchesByTable(matches: FulltextMatch[]): GroupedMatches[] {
  const groups = new Map<string, GroupedMatches>();

  for (const match of matches) {
    const key = `${match.namespace.database}:${match.namespace.schema ?? ''}:${match.table_name}`;

    if (!groups.has(key)) {
      groups.set(key, {
        namespace: match.namespace,
        tableName: match.table_name,
        matches: [],
      });
    }

    groups.get(key)!.matches.push(match);
  }

  return Array.from(groups.values());
}

export function FulltextSearchPanel({
  isOpen,
  onClose,
  sessionId,
  onNavigateToTable,
}: FulltextSearchPanelProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState('');
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<FulltextSearchResponse | null>(null);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const inputRef = useRef<HTMLInputElement>(null);
  const debounceRef = useRef<NodeJS.Timeout | null>(null);

  // Focus input when opened
  useEffect(() => {
    if (isOpen) {
      inputRef.current?.focus();
      setQuery('');
      setResult(null);
      setExpandedGroups(new Set());
    }
  }, [isOpen]);

  // Handle keyboard shortcuts
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (!isOpen) return;
      if (e.key === 'Escape') {
        onClose();
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  const performSearch = useCallback(async (searchTerm: string) => {
    if (!sessionId || searchTerm.trim().length < 2) {
      setResult(null);
      return;
    }

    setLoading(true);
    try {
      const response = await fulltextSearch(sessionId, searchTerm, {
        case_sensitive: caseSensitive,
        max_results_per_table: 10,
        max_total_results: 100,
      });
      setResult(response);

      // Auto-expand all groups with matches
      if (response.success && response.matches.length > 0) {
        const groups = groupMatchesByTable(response.matches);
        setExpandedGroups(new Set(groups.map(g =>
          `${g.namespace.database}:${g.namespace.schema ?? ''}:${g.tableName}`
        )));
      }
    } catch (err) {
      setResult({
        success: false,
        matches: [],
        total_matches: 0,
        tables_searched: 0,
        search_time_ms: 0,
        error: err instanceof Error ? err.message : 'Unknown error',
        truncated: false,
      });
    } finally {
      setLoading(false);
    }
  }, [sessionId, caseSensitive]);

  const handleSearchChange = useCallback((value: string) => {
    setQuery(value);

    // Clear previous debounce
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
    }

    // Debounce the search
    debounceRef.current = setTimeout(() => {
      performSearch(value);
    }, 300);
  }, [performSearch]);

  const toggleGroup = useCallback((key: string) => {
    setExpandedGroups(prev => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  }, []);

  const handleTableClick = useCallback((namespace: Namespace, tableName: string, columnName: string) => {
    const filter: SearchFilter = {
      column: columnName,
      value: query,
      caseSensitive,
    };
    onNavigateToTable?.(namespace, tableName, filter);
    onClose();
  }, [onNavigateToTable, onClose, query, caseSensitive]);

  if (!isOpen) return null;

  const groupedMatches = result?.success ? groupMatchesByTable(result.matches) : [];

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[10vh] bg-background/80 backdrop-blur-sm p-4"
      onClick={onClose}
    >
      <div
        className="w-full max-w-2xl bg-background border border-border rounded-lg shadow-2xl overflow-hidden flex flex-col ring-1 ring-border max-h-[80vh]"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border bg-muted/30">
          <div className="flex items-center gap-2">
            <Database className="w-5 h-5 text-primary" />
            <h2 className="font-semibold">{t('fulltextSearch.title')}</h2>
          </div>
          <Button variant="ghost" size="icon" onClick={onClose}>
            <X className="w-4 h-4" />
          </Button>
        </div>

        {/* Search input */}
        <div className="p-4 border-b border-border space-y-3">
          <div className="flex items-center gap-2">
            <div className="relative flex-1">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
              <Input
                ref={inputRef}
                className="pl-9 pr-4"
                type="text"
                placeholder={t('fulltextSearch.placeholder')}
                value={query}
                onChange={(e) => handleSearchChange(e.target.value)}
              />
            </div>
            {loading && (
              <Loader2 className="w-5 h-5 animate-spin text-primary" />
            )}
          </div>

          <div className="flex items-center gap-4 text-sm">
            <div className="flex items-center gap-2">
              <Switch
                id="case-sensitive"
                checked={caseSensitive}
                onCheckedChange={(checked) => {
                  setCaseSensitive(checked);
                  if (query.trim().length >= 2) {
                    performSearch(query);
                  }
                }}
              />
              <Label htmlFor="case-sensitive" className="text-muted-foreground">
                {t('fulltextSearch.caseSensitive')}
              </Label>
            </div>
          </div>
        </div>

        {/* Results */}
        <div className="flex-1 overflow-y-auto">
          {!sessionId ? (
            <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
              <AlertCircle className="w-8 h-8 mb-2" />
              <p>{t('fulltextSearch.noConnection')}</p>
            </div>
          ) : result?.error ? (
            <div className="flex flex-col items-center justify-center py-12 text-destructive">
              <AlertCircle className="w-8 h-8 mb-2" />
              <p>{result.error}</p>
            </div>
          ) : result?.success && groupedMatches.length === 0 && query.trim().length >= 2 ? (
            <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
              <Search className="w-8 h-8 mb-2 opacity-50" />
              <p>{t('fulltextSearch.noResults')}</p>
              <p className="text-xs mt-1">
                {t('fulltextSearch.searchedTables', { count: result.tables_searched })}
              </p>
            </div>
          ) : groupedMatches.length > 0 ? (
            <div className="py-2">
              {/* Stats bar */}
              <div className="px-4 py-2 text-xs text-muted-foreground border-b border-border bg-muted/20">
                {t('fulltextSearch.stats', {
                  matches: result!.total_matches,
                  tables: result!.tables_searched,
                  time: result!.search_time_ms.toFixed(0),
                })}
                {result!.truncated && (
                  <span className="ml-2 text-amber-500">
                    ({t('fulltextSearch.truncated')})
                  </span>
                )}
              </div>

              {/* Grouped results */}
              {groupedMatches.map((group) => {
                const key = `${group.namespace.database}:${group.namespace.schema ?? ''}:${group.tableName}`;
                const isExpanded = expandedGroups.has(key);

                return (
                  <div key={key} className="border-b border-border last:border-b-0">
                    {/* Group header */}
                    <button
                      className="w-full flex items-center gap-2 px-4 py-2.5 hover:bg-muted/50 transition-colors text-left"
                      onClick={() => toggleGroup(key)}
                    >
                      <ChevronRight
                        className={cn(
                          'w-4 h-4 text-muted-foreground transition-transform',
                          isExpanded && 'rotate-90'
                        )}
                      />
                      <Table2 className="w-4 h-4 text-primary" />
                      <span className="font-medium">
                        {group.namespace.schema
                          ? `${group.namespace.database}.${group.namespace.schema}.${group.tableName}`
                          : `${group.namespace.database}.${group.tableName}`}
                      </span>
                      <span className="text-xs text-muted-foreground ml-auto">
                        {group.matches.length} {group.matches.length === 1 ? 'match' : 'matches'}
                      </span>
                    </button>

                    {/* Matches */}
                    {isExpanded && (
                      <div className="bg-muted/20">
                        {group.matches.map((match, idx) => (
                          <div
                            key={idx}
                            className="px-4 py-2 pl-10 border-t border-border/50 hover:bg-muted/30 cursor-pointer"
                            onClick={() => handleTableClick(match.namespace, match.table_name, match.column_name)}
                          >
                            <div className="flex items-center gap-2 text-sm">
                              <span className="text-muted-foreground font-mono text-xs">
                                {match.column_name}:
                              </span>
                              <span className="truncate">
                                <HighlightedText
                                  text={match.value_preview}
                                  highlight={query}
                                  caseSensitive={caseSensitive}
                                />
                              </span>
                            </div>
                            {/* Row preview */}
                            <div className="flex flex-wrap gap-x-3 gap-y-1 mt-1 text-xs text-muted-foreground">
                              {match.row_preview.slice(0, 4).map(([colName, value]) => (
                                <span key={colName} className="truncate max-w-[150px]">
                                  <span className="font-medium">{colName}:</span>{' '}
                                  {formatValue(value)}
                                </span>
                              ))}
                              {match.row_preview.length > 4 && (
                                <span className="italic">+{match.row_preview.length - 4} more</span>
                              )}
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
              <Search className="w-8 h-8 mb-2 opacity-50" />
              <p>{t('fulltextSearch.hint')}</p>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between px-4 py-2 border-t border-border bg-muted/20 text-xs text-muted-foreground">
          <span>{t('fulltextSearch.footerHint')}</span>
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

// Helper component to highlight search terms
function HighlightedText({
  text,
  highlight,
  caseSensitive,
}: {
  text: string;
  highlight: string;
  caseSensitive: boolean;
}) {
  if (!highlight.trim()) {
    return <>{text}</>;
  }

  const regex = new RegExp(
    `(${highlight.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`,
    caseSensitive ? 'g' : 'gi'
  );

  const parts = text.split(regex);

  return (
    <>
      {parts.map((part, i) => {
        const isMatch = caseSensitive
          ? part === highlight
          : part.toLowerCase() === highlight.toLowerCase();

        return isMatch ? (
          <mark key={i} className="bg-yellow-200 dark:bg-yellow-800 rounded px-0.5">
            {part}
          </mark>
        ) : (
          <span key={i}>{part}</span>
        );
      })}
    </>
  );
}
