// SPDX-License-Identifier: BUSL-1.1

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { BookmarkPlus, History, Layers, Loader2, Network, Play, Square, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Tooltip } from '@/components/ui/tooltip';
import { Driver } from '@/lib/drivers';
import {
  buildAliasMap,
  buildAliasSet,
  executeFederationQuery,
  type FederationSource,
  isFederationQuery,
  listFederationSources,
} from '@/lib/federation';
import type { SavedConnection, Namespace } from '@/lib/tauri';
import { cancelQuery } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { getModifierKey } from '@/utils/platform';
import { FederationSourceBar } from './FederationSourceBar';
import { FederationEmptyState } from './FederationEmptyState';
import { SQLEditor, type SQLEditorHandle } from '../Editor/SQLEditor';
import { QueryPanelResults, type QueryResultEntry } from '../Query/QueryPanelResults';
import { ConnectionModal } from '../Connection/ConnectionModal';

// ============================================
// SMART INSERT — Query context analysis
// ============================================

type InsertAction = 'select-from' | 'join' | 'left-join' | 'comma' | 'where' | 'raw';

interface InsertOption {
  action: InsertAction;
  labelKey: string;
  kbd?: string;
}

function analyzeQueryContext(query: string): InsertAction | 'choose' {
  const trimmed = query.trim();
  if (!trimmed) return 'select-from';

  const upper = trimmed.toUpperCase();
  const hasSelect = /\bSELECT\b/.test(upper);
  const hasFrom = /\bFROM\s+\S+/.test(upper);

  // Has a SELECT...FROM with table(s) → offer JOIN options
  if (hasSelect && hasFrom) return 'choose';

  // Has SELECT but no FROM yet → complete with FROM
  if (hasSelect && !hasFrom) return 'select-from';

  // Has content but we can't parse it well → offer choices
  if (trimmed.length > 5) return 'choose';

  return 'select-from';
}

function getInsertOptions(query: string): InsertOption[] {
  const upper = query.trim().toUpperCase();
  const hasWhere = /\bWHERE\b/.test(upper);
  const hasFrom = /\bFROM\s+\S+/.test(upper);

  const options: InsertOption[] = [];

  if (hasFrom) {
    options.push({ action: 'join', labelKey: 'federation.insertJoin', kbd: 'J' });
    options.push({ action: 'left-join', labelKey: 'federation.insertLeftJoin', kbd: 'L' });
    options.push({ action: 'comma', labelKey: 'federation.insertComma' });
  }

  if (hasFrom && !hasWhere) {
    options.push({ action: 'where', labelKey: 'federation.insertWhere', kbd: 'W' });
  }

  options.push({ action: 'raw', labelKey: 'federation.insertRaw' });

  return options;
}

function buildInsertText(action: InsertAction, tablePath: string, query: string): string {
  const alias = tablePath.split('.').pop()?.[0]?.toLowerCase() || 't';
  switch (action) {
    case 'select-from': {
      const trimmed = query.trim();
      if (!trimmed) return `SELECT * FROM ${tablePath}`;
      // Has SELECT but no FROM → append FROM
      if (/\bSELECT\b/i.test(trimmed) && !/\bFROM\b/i.test(trimmed)) {
        return `\nFROM ${tablePath}`;
      }
      return `SELECT * FROM ${tablePath}`;
    }
    case 'join':
      return `\nJOIN ${tablePath} ${alias} ON `;
    case 'left-join':
      return `\nLEFT JOIN ${tablePath} ${alias} ON `;
    case 'comma':
      return `, ${tablePath}`;
    case 'where':
      return `\nWHERE ${tablePath}.`;
    case 'raw':
      return tablePath;
  }
}

// ============================================
// COMPONENT
// ============================================

interface FederationViewerProps {
  activeConnection?: SavedConnection | null;
  initialQuery?: string;
  isActive?: boolean;
}

export function FederationViewer({ initialQuery = '' }: FederationViewerProps) {
  const { t } = useTranslation();

  // --- Sources ---
  const [sources, setSources] = useState<FederationSource[]>([]);
  const [sourcesLoading, setSourcesLoading] = useState(false);

  // --- Query state ---
  const [query, setQuery] = useState(initialQuery);
  const [results, setResults] = useState<QueryResultEntry[]>([]);
  const [activeResultId, setActiveResultId] = useState<string | null>(null);
  const [keepResults, setKeepResults] = useState(false);

  // --- Execution state ---
  const [loading, setLoading] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [activeQueryId, setActiveQueryId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // --- Smart insert state ---
  const [pendingInsert, setPendingInsert] = useState<string | null>(null);

  // --- Refs ---
  const sqlEditorRef = useRef<SQLEditorHandle>(null);
  const [connectionModalOpen, setConnectionModalOpen] = useState(false);

  // --- Load sources ---
  const loadSources = useCallback(async () => {
    setSourcesLoading(true);
    try {
      const activeSources = await listFederationSources();
      setSources(activeSources);
    } catch (err) {
      console.error('Failed to load federation sources:', err);
    } finally {
      setSourcesLoading(false);
    }
  }, []);

  useEffect(() => {
    loadSources();
  }, [loadSources]);

  // --- Federation detection ---
  const aliasSet = useMemo(() => buildAliasSet(sources), [sources]);
  const isFederated = useMemo(
    () => sources.length >= 2 && isFederationQuery(query, aliasSet),
    [query, sources, aliasSet]
  );

  const hasEnoughSources = sources.length >= 2;
  const hasResults = results.length > 0;

  // --- Execute ---
  const handleExecute = useCallback(
    async (queryToRun: string = query) => {
      if (!queryToRun.trim()) return;

      if (!hasEnoughSources) {
        toast.error(t('federation.needTwoSources'));
        return;
      }

      setLoading(true);
      setError(null);
      setPendingInsert(null);
      const queryId = `fed-${Date.now()}`;
      setActiveQueryId(queryId);

      const startTime = performance.now();

      try {
        const aliasMap = buildAliasMap(sources);
        const result = await executeFederationQuery(queryToRun, aliasMap, { queryId });
        const duration = performance.now() - startTime;

        const newEntry: QueryResultEntry = {
          id: queryId,
          kind: 'query',
          query: queryToRun,
          result: result.result,
          error: result.error,
          executedAt: Date.now(),
          totalTimeMs: duration,
        };

        setResults(prev => (keepResults ? [newEntry, ...prev] : [newEntry]));
        setActiveResultId(queryId);

        if (result.success) {
          toast.success(t('query.success', { time: Math.round(duration) }));
        } else if (result.error) {
          setError(result.error);
          toast.error(result.error);
        }
      } catch (err: any) {
        const duration = performance.now() - startTime;
        const newEntry: QueryResultEntry = {
          id: queryId,
          kind: 'query',
          query: queryToRun,
          error: err.message || String(err),
          executedAt: Date.now(),
          totalTimeMs: duration,
        };
        setResults(prev => (keepResults ? [newEntry, ...prev] : [newEntry]));
        setActiveResultId(queryId);
        setError(err.message || String(err));
        toast.error(err.message || String(err));
      } finally {
        setLoading(false);
        setActiveQueryId(null);
        setCancelling(false);
      }
    },
    [query, sources, hasEnoughSources, keepResults, t]
  );

  // --- Cancel ---
  const handleCancel = useCallback(async () => {
    if (!activeQueryId || cancelling) return;
    setCancelling(true);
    try {
      await cancelQuery(activeQueryId);
    } catch (err: any) {
      toast.error(err.message || 'Failed to cancel query');
      setCancelling(false);
    }
  }, [activeQueryId, cancelling]);

  // --- Smart insert: table clicked from source bar ---
  const handleInsertTable = useCallback(
    (alias: string, ns: Namespace, table: string) => {
      if (!sqlEditorRef.current) return;
      // Federation references use:
      // - alias.database.table
      // - alias.database.schema.table
      const tablePath = ns.schema
        ? `${alias}.${ns.database}.${ns.schema}.${table}`
        : `${alias}.${ns.database}.${table}`;

      const context = analyzeQueryContext(query);

      if (context === 'choose') {
        // Show the action bar for the user to choose
        setPendingInsert(tablePath);
      } else {
        // Direct insert (empty editor or simple case)
        const text = buildInsertText(context, tablePath, query);
        if (!query.trim()) {
          // Replace entire editor content
          setQuery(text);
        } else {
          sqlEditorRef.current.insertSnippet(text);
        }
        sqlEditorRef.current.focus();
      }
    },
    [query]
  );

  // --- Smart insert: action chosen from action bar ---
  const handleInsertAction = useCallback(
    (action: InsertAction) => {
      if (!pendingInsert || !sqlEditorRef.current) return;
      const text = buildInsertText(action, pendingInsert, query);

      if (action === 'select-from' && !query.trim()) {
        setQuery(text);
      } else {
        sqlEditorRef.current.insertSnippet(text);
      }

      setPendingInsert(null);
      sqlEditorRef.current.focus();
    },
    [pendingInsert, query]
  );

  // --- Keyboard shortcuts for action bar ---
  useEffect(() => {
    if (!pendingInsert) return;
    const handler = (e: KeyboardEvent) => {
      const key = e.key.toUpperCase();
      if (key === 'ESCAPE') {
        setPendingInsert(null);
        sqlEditorRef.current?.focus();
        return;
      }
      if (key === 'J') {
        e.preventDefault();
        handleInsertAction('join');
      } else if (key === 'L') {
        e.preventDefault();
        handleInsertAction('left-join');
      } else if (key === 'W') {
        e.preventDefault();
        handleInsertAction('where');
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [pendingInsert, handleInsertAction]);

  // --- Insert example query ---
  const handleTryExample = useCallback((exampleQuery: string) => {
    setQuery(exampleQuery);
    setTimeout(() => sqlEditorRef.current?.focus(), 50);
  }, []);

  // --- Connection change listener ---
  useEffect(() => {
    const handler = () => loadSources();
    window.addEventListener('qoredb:connections-changed', handler);
    return () => window.removeEventListener('qoredb:connections-changed', handler);
  }, [loadSources]);

  // --- Insert options for pending insert ---
  const insertOptions = useMemo(
    () => (pendingInsert ? getInsertOptions(query) : []),
    [pendingInsert, query]
  );

  return (
    <div className="flex flex-col w-full h-full bg-background overflow-hidden">
      {/* Source bar */}
      <FederationSourceBar
        sources={sources}
        loading={sourcesLoading}
        onRefresh={loadSources}
        onAddSource={() => setConnectionModalOpen(true)}
        onInsertTable={handleInsertTable}
      />

      {/* Smart insert action bar */}
      {pendingInsert && (
        <div className="flex items-center gap-2 px-3 py-1.5 border-b border-accent/20 bg-accent/5 shrink-0 animate-in fade-in slide-in-from-top-1 duration-150">
          <span className="text-xs text-foreground">
            {t('federation.insertAs')}
            <code className="ml-1.5 font-mono text-accent bg-accent/10 px-1.5 py-0.5 rounded text-[11px]">
              {pendingInsert}
            </code>
          </span>

          <div className="flex items-center gap-1 ml-2">
            {insertOptions.map(opt => (
              <Button
                key={opt.action}
                variant="outline"
                size="sm"
                className="h-6 text-[11px] px-2 gap-1.5 border-accent/20 hover:bg-accent/10 hover:border-accent/30"
                onClick={() => handleInsertAction(opt.action)}
              >
                {t(opt.labelKey)}
                {opt.kbd && (
                  <kbd className="ml-0.5 text-[9px] font-mono text-muted-foreground bg-muted/60 px-1 py-px rounded">
                    {opt.kbd}
                  </kbd>
                )}
              </Button>
            ))}
          </div>

          <button
            onClick={() => {
              setPendingInsert(null);
              sqlEditorRef.current?.focus();
            }}
            className="ml-auto p-1 text-muted-foreground hover:text-foreground rounded transition-colors"
          >
            <X size={12} />
          </button>
        </div>
      )}

      {/* Toolbar */}
      <div className="flex items-center gap-2 px-3 py-1.5 border-b border-border bg-muted/20 shrink-0">
        <Button
          onClick={() => handleExecute(query)}
          disabled={loading || !hasEnoughSources || !query.trim()}
          className="gap-2"
          size="sm"
        >
          {loading ? (
            <span className="flex items-center gap-2">
              <Loader2 size={14} className="animate-spin" />
              {t('query.running')}
            </span>
          ) : (
            <>
              <Play size={14} className="fill-current" /> {t('query.run')}
            </>
          )}
        </Button>

        {loading && (
          <Button
            variant="destructive"
            size="sm"
            onClick={handleCancel}
            disabled={cancelling}
            className="gap-2"
          >
            <Square size={14} className="fill-current" /> {t('query.stop')}
          </Button>
        )}

        {/* Federation status badge */}
        {hasEnoughSources && query.trim() && (
          <span
            className={cn(
              'flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full border transition-colors',
              isFederated
                ? 'text-accent bg-accent/10 border-accent/20'
                : 'text-muted-foreground bg-muted/40 border-transparent'
            )}
          >
            <Network size={10} />
            {isFederated ? t('federation.detected') : t('federation.singleSource')}
          </span>
        )}

        {!hasEnoughSources && sources.length > 0 && (
          <span className="text-[11px] text-muted-foreground">
            {t('federation.needTwoSources')}
          </span>
        )}

        <div className="flex-1" />

        {/* Keep results toggle */}
        <Tooltip content={t('query.keepResults')}>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => setKeepResults(prev => !prev)}
            className={cn(
              'h-7 w-7',
              keepResults
                ? 'text-accent bg-accent/10 hover:bg-accent/20'
                : 'text-muted-foreground hover:text-foreground'
            )}
          >
            <Layers size={14} />
          </Button>
        </Tooltip>

        <Tooltip content={t('query.history')}>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-muted-foreground hover:text-foreground"
            disabled
          >
            <History size={14} />
          </Button>
        </Tooltip>

        <Tooltip content={t('library.save')}>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-muted-foreground hover:text-foreground"
            disabled
          >
            <BookmarkPlus size={14} />
          </Button>
        </Tooltip>

        <span className="text-[10px] text-muted-foreground hidden sm:inline-block ml-1">
          {t('query.runHint', { modifier: getModifierKey() })}
        </span>
      </div>

      {/* Editor */}
      <div className="min-h-30 h-50 border-b border-border flex flex-col relative shrink-0">
        <SQLEditor
          ref={sqlEditorRef}
          value={query}
          onChange={setQuery}
          onExecute={() => handleExecute(query)}
          onExecuteSelection={sel => handleExecute(sel)}
          dialect={Driver.Postgres}
          readOnly={loading}
          placeholder={t('federation.editorPlaceholder')}
        />
      </div>

      {/* Results or Empty State */}
      <div className="flex-1 min-h-0 flex flex-col">
        {hasResults ? (
          <QueryPanelResults
            panelError={error}
            results={results}
            activeResultId={activeResultId}
            isDocumentBased={false}
            sessionId={null}
            environment="development"
            readOnly={true}
            query={query}
            onSelectResult={setActiveResultId}
            onCloseResult={id => {
              setResults(prev => prev.filter(r => r.id !== id));
              if (activeResultId === id) {
                const remaining = results.filter(r => r.id !== id);
                setActiveResultId(remaining[0]?.id || null);
              }
            }}
            onRowsDeleted={() => {}}
            onEditDocument={() => {}}
          />
        ) : (
          <FederationEmptyState
            sources={sources}
            hasEnoughSources={hasEnoughSources}
            loading={loading}
            onAddSource={() => setConnectionModalOpen(true)}
            onTryExample={handleTryExample}
          />
        )}
      </div>

      {connectionModalOpen && (
        <ConnectionModal
          isOpen={true}
          onClose={() => {
            setConnectionModalOpen(false);
            loadSources();
          }}
          onConnected={async () => {
            setConnectionModalOpen(false);
            loadSources();
          }}
        />
      )}
    </div>
  );
}
