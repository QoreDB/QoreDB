import { useState, useCallback } from 'react';
import { SQLEditor } from '../Editor/SQLEditor';
import { ResultsTable } from '../Results/ResultsTable';
import { executeQuery, cancelQuery, QueryResult } from '../../lib/tauri';
import './QueryPanel.css';

interface QueryPanelProps {
  sessionId: string | null;
  dialect?: 'postgres' | 'mysql' | 'mongodb';
}

export function QueryPanel({ sessionId, dialect = 'postgres' }: QueryPanelProps) {
  const [query, setQuery] = useState('SELECT 1;');
  const [result, setResult] = useState<QueryResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleExecute = useCallback(async (sql?: string) => {
    if (!sessionId) {
      setError('No connection selected');
      return;
    }

    const queryToRun = sql || query;
    if (!queryToRun.trim()) return;

    setLoading(true);
    setError(null);
    setResult(null);

    try {
      const response = await executeQuery(sessionId, queryToRun);
      
      if (response.success && response.result) {
        setResult(response.result);
      } else {
        setError(response.error || 'Query failed');
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, [sessionId, query]);

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
    <div className="query-panel">
      {/* Toolbar */}
      <div className="query-toolbar">
        <button
          className="query-btn primary"
          onClick={() => handleExecute()}
          disabled={loading || !sessionId}
        >
          {loading ? '⏳ Running...' : '▶ Run'}
        </button>

        {loading && (
          <button
            className="query-btn danger"
            onClick={handleCancel}
            disabled={cancelling}
          >
            {cancelling ? 'Stopping...' : '⏹ Stop'}
          </button>
        )}

        <span className="query-hint">
          Cmd+Enter to run • Select text to run partial
        </span>

        {!sessionId && (
          <span className="query-warning">⚠ No connection</span>
        )}
      </div>

      {/* Editor */}
      <div className="query-editor">
        <SQLEditor
          value={query}
          onChange={setQuery}
          onExecute={() => handleExecute()}
          onExecuteSelection={(selection) => handleExecute(selection)}
          dialect={dialect}
          readOnly={loading}
        />
      </div>

      {/* Results / Error */}
      <div className="query-results">
        {error && (
          <div className="query-error">
            <span className="error-icon">✕</span>
            {error}
          </div>
        )}

        {!error && (
          <ResultsTable result={result} height={300} />
        )}
      </div>
    </div>
  );
}
