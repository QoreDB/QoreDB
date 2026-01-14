import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  getHistory, 
  searchHistory, 
  removeFromHistory, 
  clearHistory,
  toggleFavorite,
  isFavorite,
  getFavorites,
  getSessionHistory,
  HistoryEntry
} from '../../lib/history';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { 
  History, 
  Star, 
  Search, 
  Trash2, 
  Play, 
  Clock,
  AlertCircle,
  CheckCircle2,
  X
} from 'lucide-react';

interface QueryHistoryProps {
  isOpen: boolean;
  onClose: () => void;
  onSelectQuery: (query: string) => void;
  sessionId?: string;
}

type Tab = 'history' | 'favorites';

export function QueryHistory({ isOpen, onClose, onSelectQuery, sessionId }: QueryHistoryProps) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>('history');
  const [search, setSearch] = useState('');
  const [entries, setEntries] = useState<HistoryEntry[]>([]);

  useEffect(() => {
    if (!isOpen) return;
    loadEntries();
  }, [isOpen, tab, search, sessionId]);

  function loadEntries() {
    if (tab === 'favorites') {
      setEntries(getFavorites());
    } else if (search) {
      setEntries(searchHistory(search));
    } else {
      if (sessionId) {
        setEntries(getSessionHistory(sessionId));
      } else {
        setEntries(getHistory());
      }
    }
  }

  function handleSelectQuery(entry: HistoryEntry) {
    onSelectQuery(entry.query);
    onClose();
  }

  function handleToggleFavorite(id: string) {
    toggleFavorite(id);
    loadEntries();
  }

  function handleRemove(id: string) {
    removeFromHistory(id);
    loadEntries();
  }

  function handleClearAll() {
    if (confirm('Clear all query history?')) {
      clearHistory();
      loadEntries();
    }
  }

  function formatTime(timestamp: number): string {
    const date = new Date(timestamp);
    const now = new Date();
    const diffMs = now.getTime() - timestamp;
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return 'just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays < 7) return `${diffDays}d ago`;
    return date.toLocaleDateString();
  }

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="w-full max-w-2xl max-h-[80vh] bg-background border border-border rounded-lg shadow-xl flex flex-col overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border">
          <div className="flex items-center gap-2">
            <History size={18} className="text-accent" />
            <h2 className="font-semibold">Query History</h2>
          </div>
          <Button variant="ghost" size="icon" onClick={onClose} className="h-8 w-8">
            <X size={16} />
          </Button>
        </div>

        {/* Tabs & Search */}
        <div className="flex items-center gap-2 px-4 py-2 border-b border-border bg-muted/20">
          <div className="flex items-center gap-1">
            <button
              className={cn(
                "px-3 py-1.5 text-sm font-medium rounded-md transition-colors",
                tab === 'history'
                  ? "bg-accent text-accent-foreground"
                  : "text-muted-foreground hover:text-foreground hover:bg-muted"
              )}
              onClick={() => setTab('history')}
            >
              <span className="flex items-center gap-1.5">
                <Clock size={14} />
                Recent
              </span>
            </button>
            <button
              className={cn(
                "px-3 py-1.5 text-sm font-medium rounded-md transition-colors",
                tab === 'favorites'
                  ? "bg-accent text-accent-foreground"
                  : "text-muted-foreground hover:text-foreground hover:bg-muted"
              )}
              onClick={() => setTab('favorites')}
            >
              <span className="flex items-center gap-1.5">
                <Star size={14} />
                Favorites
              </span>
            </button>
          </div>

          <div className="flex-1" />

          {/* Search */}
          <div className="relative">
            <Search size={14} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
            <input
              type="text"
              placeholder="Search queries..."
              value={search}
              onChange={e => setSearch(e.target.value)}
              className="h-8 pl-8 pr-3 text-sm rounded-md border border-input bg-background focus:outline-none focus:ring-1 focus:ring-ring w-48"
            />
          </div>

          {tab === 'history' && entries.length > 0 && (
            <Button
              variant="ghost"
              size="sm"
              onClick={handleClearAll}
              className="h-8 text-xs text-muted-foreground hover:text-error"
            >
              <Trash2 size={14} className="mr-1" />
              Clear
            </Button>
          )}
        </div>

        {/* Entries List */}
        <div className="flex-1 overflow-auto">
          {entries.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-48 text-muted-foreground">
              <History size={32} className="mb-2 opacity-50" />
              <p className="text-sm">
                {tab === 'favorites' ? 'No favorite queries yet' : 'No query history'}
              </p>
            </div>
          ) : (
            <div className="divide-y divide-border">
              {entries.map(entry => (
                <div
                  key={entry.id}
                  className="group flex items-start gap-3 px-4 py-3 hover:bg-muted/30 transition-colors"
                >
                  {/* Status Icon */}
                  <div className="mt-1">
                    {entry.error ? (
                      <AlertCircle size={14} className="text-error" />
                    ) : (
                      <CheckCircle2 size={14} className="text-green-500" />
                    )}
                  </div>

                  {/* Query Content */}
                  <div className="flex-1 min-w-0">
                    <pre className="font-mono text-xs text-foreground whitespace-pre-wrap break-all line-clamp-3">
                      {entry.query}
                    </pre>
                    <div className="flex items-center gap-3 mt-1.5 text-xs text-muted-foreground">
                      <span>{formatTime(entry.executedAt)}</span>
                      {entry.executionTimeMs && entry.totalTimeMs ? (
                        <span title={`${t('query.time.exec')}: ${entry.executionTimeMs.toFixed(2)}ms | ${t('query.time.transfer')}: ${(entry.totalTimeMs - entry.executionTimeMs).toFixed(2)}ms`}>
                          {entry.totalTimeMs.toFixed(2)}ms
                        </span>
                      ) : entry.executionTimeMs ? (
                         <span>{entry.executionTimeMs.toFixed(2)}ms</span>
                      ) : null}
                      {entry.rowCount !== undefined && (
                        <span>{entry.rowCount} rows</span>
                      )}
                      {entry.database && (
                        <span className="font-mono">{entry.database}</span>
                      )}
                    </div>
                  </div>

                  {/* Actions */}
                  <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7"
                      onClick={() => handleSelectQuery(entry)}
                      title="Use this query"
                    >
                      <Play size={14} />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      className={cn(
                        "h-7 w-7",
                        isFavorite(entry.id) && "text-yellow-500"
                      )}
                      onClick={() => handleToggleFavorite(entry.id)}
                      title={isFavorite(entry.id) ? "Remove from favorites" : "Add to favorites"}
                    >
                      <Star size={14} className={isFavorite(entry.id) ? "fill-current" : ""} />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 text-muted-foreground hover:text-error"
                      onClick={() => handleRemove(entry.id)}
                      title="Remove from history"
                    >
                      <Trash2 size={14} />
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="px-4 py-2 border-t border-border text-xs text-muted-foreground bg-muted/10">
          {entries.length} {entries.length === 1 ? 'entry' : 'entries'}
          {search && ` matching "${search}"`}
        </div>
      </div>
    </div>
  );
}
