import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { SQLEditor } from '../Editor/SQLEditor';
import { MongoEditor, MONGO_TEMPLATES } from '../Editor/MongoEditor';
import { DataGrid } from '../Grid/DataGrid';
import { JSONViewer } from '../Results/JSONViewer';
import { QueryHistory } from '../History/QueryHistory';
import { executeQuery, cancelQuery, QueryResult } from '../../lib/tauri';
import { addToHistory } from '../../lib/history';
import { logError } from '../../lib/errorLog';
import { Button } from '@/components/ui/button';
import { Play, Square, AlertCircle, History } from 'lucide-react';

import { Driver } from '../../lib/drivers';

interface QueryPanelProps {
  sessionId: string | null;
  dialect?: Driver;
}

export function QueryPanel({ sessionId, dialect = 'postgres' }: QueryPanelProps) {
  const { t } = useTranslation();
  const isMongo = dialect === 'mongodb';
  const defaultQuery = isMongo ? MONGO_TEMPLATES.find : 'SELECT 1;';
  
  const [query, setQuery] = useState(defaultQuery);
  const [result, setResult] = useState<QueryResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);

  const handleExecute = useCallback(async (queryText?: string) => {
    if (!sessionId) {
      setError(t('query.noConnectionError'));
      return;
    }

    const queryToRun = queryText || query;
    if (!queryToRun.trim()) return;

    setLoading(true);
    setError(null);
    setResult(null);

    const startTime = performance.now();
    try {
      const response = await executeQuery(sessionId, queryToRun);
      const endTime = performance.now();
      const totalTime = endTime - startTime;
      
      if (response.success && response.result) {
        const enrichedResult = {
          ...response.result,
          total_time_ms: totalTime
        };
        setResult(enrichedResult);
        
        // Save to history
        addToHistory({
          query: queryToRun,
          sessionId,
          driver: dialect,
          executedAt: Date.now(),
          executionTimeMs: response.result.execution_time_ms,
          totalTimeMs: totalTime,
          rowCount: response.result.rows.length,
        });
      } else {
        setError(response.error || t('query.queryFailed'));
        // Save failed query to history
        addToHistory({
          query: queryToRun,
          sessionId,
          driver: dialect,
          executedAt: Date.now(),
          executionTimeMs: 0,
          totalTimeMs: totalTime, 
          error: response.error || t('query.queryFailed'),
        });
        logError('QueryPanel', response.error || t('query.queryFailed'), queryToRun, sessionId);
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : t('common.error');
      setError(errorMessage);
      logError('QueryPanel', errorMessage, queryToRun, sessionId || undefined);
    } finally {
      setLoading(false);
    }
  }, [sessionId, query, dialect, t]);

  const handleCancel = useCallback(async () => {
    if (!sessionId || !loading) return;

    setCancelling(true);
    try {
      await cancelQuery(sessionId);
    } catch (err) {
      console.error('Failed to cancel:', err);
    } finally {
      setCancelling(false);
      setLoading(false);
    }
  }, [sessionId, loading]);

  return (
    <div className="flex flex-col h-full bg-background rounded-lg border border-border shadow-sm overflow-hidden">
      {/* Toolbar */}
      <div className="flex items-center gap-2 p-2 border-b border-border bg-muted/20">
        <Button
          onClick={() => handleExecute()}
          disabled={loading || !sessionId}
          className="w-24 gap-2"
        >
          {loading ? (
             <span className="flex items-center gap-2">{t('query.running')}</span>
          ) : (
            <>
              <Play size={16} className="fill-current" /> {t('query.run')}
            </>
          )}
        </Button>

        {loading && (
          <Button
            variant="destructive"
            onClick={handleCancel}
            disabled={cancelling}
            className="w-24 gap-2"
          >
            <Square size={16} className="fill-current" /> {t('query.stop')}
          </Button>
        )}

        {isMongo && (
          <select
            className="h-9 rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            onChange={(e) => setQuery(MONGO_TEMPLATES[e.target.value as keyof typeof MONGO_TEMPLATES] || query)}
            defaultValue=""
          >
            <option value="" disabled>Templates...</option>
            <option value="find">find()</option>
            <option value="findOne">findOne()</option>
            <option value="aggregate">aggregate()</option>
            <option value="insertOne">insertOne()</option>
            <option value="updateOne">updateOne()</option>
            <option value="deleteOne">deleteOne()</option>
          </select>
        )}

        <div className="flex-1" />

        <Button
          variant="ghost"
          size="sm"
          onClick={() => setHistoryOpen(true)}
          className="h-9 px-2 text-muted-foreground hover:text-foreground"
          title={t('query.history')}
        >
          <History size={16} className="mr-1" />
          {t('query.history')}
        </Button>


        <span className="text-xs text-muted-foreground hidden sm:inline-block">
          {t('query.runHint')}
        </span>

        {!sessionId && (
          <span className="flex items-center gap-1.5 text-xs text-warning bg-warning/10 px-2 py-1 rounded-full border border-warning/20">
            <AlertCircle size={12} /> {t('query.noConnection')}
          </span>
        )}
      </div>

      <div className="flex-1 min-h-[200px] border-b border-border relative">
        {isMongo ? (
          <MongoEditor
            value={query}
            onChange={setQuery}
            onExecute={() => handleExecute()}
            readOnly={loading}
          />
        ) : (
          <SQLEditor
            value={query}
            onChange={setQuery}
            onExecute={() => handleExecute()}
            onExecuteSelection={(selection) => handleExecute(selection)}
            dialect={dialect as 'postgres' | 'mysql'}
            readOnly={loading}
          />
        )}
      </div>

      {/* Results / Error */}
      <div className="flex-1 flex flex-col min-h-0 bg-background overflow-hidden relative">
        {error ? (
          <div className="p-4 m-4 rounded-md bg-error/10 border border-error/20 text-error flex items-start gap-3">
            <AlertCircle className="mt-0.5 shrink-0" size={18} />
            <pre className="text-sm font-mono whitespace-pre-wrap break-all">{error}</pre>
          </div>
        ) : result ? (
          isMongo ? (
            <JSONViewer data={result.rows.map(r => r.values[0])} />
          ) : (
            <div className="flex-1 overflow-hidden p-2 flex flex-col h-full">
               {/* DataGrid fills container */}
               <DataGrid result={result} />
            </div>
          )
        ) : (
          <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
            {t('query.noResults')}
          </div>
        )}
      </div>

      {/* History Modal */}
      <QueryHistory
        isOpen={historyOpen}
        onClose={() => setHistoryOpen(false)}
        onSelectQuery={setQuery}
        sessionId={sessionId || undefined}
      />
    </div>
  );
}

