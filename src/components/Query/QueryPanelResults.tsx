import { useTranslation } from 'react-i18next';
import { AlertCircle, X, CheckCircle2 } from 'lucide-react';
import { DataGrid } from '../Grid/DataGrid';
import { DocumentResults } from '../Results/DocumentResults';
import { ExplainPlanView } from '../Results/ExplainPlanView';
import { Environment, QueryResult, Value } from '../../lib/tauri';
import { getCollectionFromQuery } from './queryPanelUtils';
import { cn } from '@/lib/utils';

export interface QueryResultEntry {
  id: string;
  kind: 'query' | 'explain';
  query: string;
  result?: QueryResult;
  error?: string;
  executedAt: number;
  totalTimeMs?: number;
  executionTimeMs?: number;
  rowCount?: number;
}

interface QueryPanelResultsProps {
  panelError: string | null;
  results: QueryResultEntry[];
  activeResultId: string | null;
  isMongo: boolean;
  sessionId: string | null;
  connectionName?: string;
  connectionDatabase?: string;
  environment: Environment;
  readOnly: boolean;
  query: string;
  onSelectResult: (resultId: string) => void;
  onCloseResult: (resultId: string) => void;
  onRowsDeleted: () => void;
  onEditDocument: (doc: Record<string, unknown>, idValue?: Value) => void;
}

export function QueryPanelResults({
  panelError,
  results,
  activeResultId,
  isMongo,
  sessionId,
  connectionName,
  connectionDatabase,
  environment,
  readOnly,
  query,
  onSelectResult,
  onCloseResult,
  onRowsDeleted,
  onEditDocument,
}: QueryPanelResultsProps) {
  const { t } = useTranslation();
  const activeResult =
    results.find(entry => entry.id === activeResultId) || results[results.length - 1] || null;
  const activeQuery = activeResult?.query || query;
  const collection = getCollectionFromQuery(activeQuery);
  const showTabs = results.length > 1;

  return (
    <div className="flex-1 flex flex-col min-h-0 bg-background overflow-hidden relative">
      {panelError ? (
        <div className="p-4 m-4 rounded-md bg-error/10 border border-error/20 text-error flex items-start gap-3">
          <AlertCircle className="mt-0.5 shrink-0" size={18} />
          <pre className="text-sm font-mono whitespace-pre-wrap break-all">{panelError}</pre>
        </div>
      ) : activeResult ? (
        <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
          {showTabs && (
            <div className="flex items-center gap-1 px-2 py-1 border-b border-border bg-muted/20 overflow-x-auto no-scrollbar">
              {results.map(entry => {
                const label = entry.kind === 'explain' ? t('query.explain') : t('query.results');
                const isActive = entry.id === activeResultId;
                return (
                  <button
                    key={entry.id}
                    onClick={() => onSelectResult(entry.id)}
                    className={cn(
                      'group flex items-center gap-2 px-3 h-7 rounded-md text-xs transition-colors',
                      isActive
                        ? 'bg-background border border-border text-foreground shadow-sm'
                        : 'text-muted-foreground hover:text-foreground hover:bg-muted/60'
                    )}
                    title={entry.query}
                  >
                    <span className="truncate max-w-48">{label}</span>
                    <span
                      className="opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-foreground"
                      onClick={event => {
                        event.stopPropagation();
                        onCloseResult(entry.id);
                      }}
                    >
                      <X size={12} />
                    </span>
                  </button>
                );
              })}
            </div>
          )}
          <div className="shrink-0 border-b border-border bg-muted/10 px-4 py-2">
            <pre className="text-xs font-mono text-muted-foreground whitespace-pre-wrap break-all max-h-32 overflow-y-auto">
              {activeResult.query}
            </pre>
          </div>
          {activeResult.error ? (
            <div className="p-4 m-4 rounded-md bg-error/10 border border-error/20 text-error flex items-start gap-3">
              <AlertCircle className="mt-0.5 shrink-0" size={18} />
              <pre className="text-sm font-mono whitespace-pre-wrap break-all">
                {activeResult.error}
              </pre>
            </div>
          ) : activeResult.kind === 'explain' && activeResult.result ? (
            <ExplainPlanView result={activeResult.result} />
          ) : activeResult.result ? (
            isMongo ? (
              <div className="flex-1 min-h-0 flex flex-col relative">
                <DocumentResults
                  result={activeResult.result}
                  sessionId={sessionId || undefined}
                  database={connectionDatabase || 'admin'}
                  collection={collection}
                  environment={environment}
                  readOnly={readOnly}
                  connectionName={connectionName}
                  connectionDatabase={connectionDatabase}
                  onRowsDeleted={onRowsDeleted}
                  onEditDocument={onEditDocument}
                />
              </div>
            ) : (
              <div className="flex-1 min-h-0 p-2 flex flex-col">
                <DataGrid
                  result={activeResult.result}
                  sessionId={sessionId || undefined}
                  connectionName={connectionName}
                  connectionDatabase={connectionDatabase}
                  environment={environment}
                  readOnly={readOnly}
                />
              </div>
            )
          ) : (
            <div className="flex flex-col items-center justify-center h-full gap-2">
              <CheckCircle2 size={24} className="text-muted-foreground/50" />
              <span className="text-muted-foreground text-sm">{t('query.emptyResults')}</span>
              {activeResult.totalTimeMs !== undefined && (
                <span className="text-xs text-muted-foreground/70">
                  {activeResult.totalTimeMs.toFixed(0)}ms
                </span>
              )}
            </div>
          )}
        </div>
      ) : (
        <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
          {t('query.noResults')}
        </div>
      )}
    </div>
  );
}
