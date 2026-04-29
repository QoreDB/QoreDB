// SPDX-License-Identifier: Apache-2.0

import { AlertCircle, CheckCircle2, Clock, Info, Scissors, Sparkles, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { countSqlStatements } from '../../lib/environment';
import type { Environment, Namespace, QueryResult, Value } from '../../lib/tauri';
import { DataGrid } from '../Grid/DataGrid';
import { DocumentResults } from '../Results/DocumentResults';
import { ExplainPlanView } from '../Results/ExplainPlanView';
import { getCollectionFromQuery } from './queryPanelUtils';

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
  truncated?: boolean;
  truncatedTotal?: number;
  timedOut?: boolean;
}

interface QueryPanelResultsProps {
  panelError: string | null;
  results: QueryResultEntry[];
  activeResultId: string | null;
  isDocumentBased: boolean;
  sessionId: string | null;
  connectionName?: string;
  connectionDatabase?: string;
  environment: Environment;
  readOnly: boolean;
  query: string;
  activeNamespace?: Namespace | null;
  onSelectResult: (resultId: string) => void;
  onCloseResult: (resultId: string) => void;
  onRowsDeleted: () => void;
  onEditDocument: (doc: Record<string, unknown>, idValue?: Value) => void;
  onFixWithAi?: (query: string, error: string) => void;
  onOverrideLimits?: (query: string, kind: 'truncated' | 'timeout') => void;
}

export function QueryPanelResults({
  panelError,
  results,
  activeResultId,
  isDocumentBased,
  sessionId,
  connectionName,
  connectionDatabase,
  environment,
  readOnly,
  query,
  activeNamespace,
  onSelectResult,
  onCloseResult,
  onRowsDeleted,
  onEditDocument,
  onFixWithAi,
  onOverrideLimits,
}: QueryPanelResultsProps) {
  const { t } = useTranslation();
  const activeResult =
    results.find(entry => entry.id === activeResultId) || results[results.length - 1] || null;
  const activeQuery = activeResult?.query || query;
  const exportNamespace =
    activeNamespace ?? (connectionDatabase ? { database: connectionDatabase } : undefined);
  const collection = getCollectionFromQuery(activeQuery);
  const showTabs = results.length > 1;
  const statementCount = !isDocumentBased ? countSqlStatements(activeQuery) : 1;
  const showMultiStatementNotice = !isDocumentBased && statementCount > 1;

  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
      {panelError ? (
        <div className="p-4 m-4 rounded-md bg-error/10 border border-error/20 text-error flex items-start gap-3">
          <AlertCircle className="mt-0.5 shrink-0" size={18} />
          <pre className="text-sm font-mono whitespace-pre-wrap break-all">{panelError}</pre>
        </div>
      ) : activeResult ? (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          {showTabs && (
            <div className="flex items-center gap-1 px-2 py-1 border-b border-border bg-muted/20 overflow-x-auto no-scrollbar">
              {results.map(entry => {
                const baseLabel =
                  entry.kind === 'explain'
                    ? t('query.explain')
                    : entry.query?.slice(0, 30)?.trim() || t('query.results');
                const timeLabel = entry.totalTimeMs ? ` (${entry.totalTimeMs.toFixed(0)}ms)` : '';
                const label = `${baseLabel}${timeLabel}`;
                const isActive = entry.id === activeResultId;
                return (
                  <button
                    type="button"
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
          {showMultiStatementNotice && (
            <div className="shrink-0 border-b border-border bg-muted/5 px-4 py-2 text-xs text-muted-foreground flex items-start gap-2">
              <Info size={14} className="mt-0.5 shrink-0 text-muted-foreground" />
              <span>{t('query.multiStatementNotice', { count: statementCount })}</span>
            </div>
          )}
          {activeResult.truncated && activeResult.result && onOverrideLimits ? (
            <div className="shrink-0 mx-4 mt-4 p-3 rounded-md bg-warning/10 border border-warning/30 text-warning-foreground flex items-start gap-3">
              <Scissors className="mt-0.5 shrink-0 text-warning" size={16} />
              <div className="flex-1 min-w-0">
                <div className="text-sm font-medium text-warning">{t('query.truncated.title')}</div>
                <div className="text-xs text-muted-foreground mt-0.5">
                  {t('query.truncated.description', {
                    shown: activeResult.result.rows.length,
                    total: activeResult.truncatedTotal ?? activeResult.result.rows.length,
                  })}
                </div>
              </div>
              <Button
                variant="outline"
                size="sm"
                className="h-7 shrink-0 gap-1.5 text-xs border-warning/40 text-warning hover:bg-warning/10 hover:text-warning"
                onClick={() => onOverrideLimits(activeResult.query, 'truncated')}
              >
                {t('query.truncated.rerun')}
              </Button>
            </div>
          ) : null}
          {activeResult.error ? (
            <div className="p-4 m-4 rounded-md bg-error/10 border border-error/20 text-error flex items-start gap-3">
              <AlertCircle className="mt-0.5 shrink-0" size={18} />
              <div className="flex-1 min-w-0">
                <pre className="text-sm font-mono whitespace-pre-wrap break-all">
                  {activeResult.error}
                </pre>
                <div className="mt-2 flex flex-wrap gap-2">
                  {onFixWithAi && (
                    <Button
                      variant="ghost"
                      size="sm"
                      className="h-7 gap-1.5 text-xs text-accent hover:text-accent"
                      onClick={() => onFixWithAi(activeResult.query, activeResult.error!)}
                    >
                      <Sparkles size={12} />
                      {t('ai.fixWithAi')}
                    </Button>
                  )}
                  {activeResult.timedOut && onOverrideLimits && (
                    <Button
                      variant="ghost"
                      size="sm"
                      className="h-7 gap-1.5 text-xs text-warning hover:text-warning"
                      onClick={() => onOverrideLimits(activeResult.query, 'timeout')}
                    >
                      <Clock size={12} />
                      {t('query.timedOut.rerun')}
                    </Button>
                  )}
                </div>
              </div>
            </div>
          ) : activeResult.kind === 'explain' && activeResult.result ? (
            <ExplainPlanView result={activeResult.result} />
          ) : activeResult.result ? (
            isDocumentBased ? (
              <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
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
                  exportQuery={activeResult.query}
                  exportNamespace={exportNamespace}
                />
              </div>
            ) : (
              <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden p-2">
                <DataGrid
                  result={activeResult.result}
                  sessionId={sessionId || undefined}
                  namespace={exportNamespace}
                  connectionName={connectionName}
                  connectionDatabase={connectionDatabase}
                  environment={environment}
                  readOnly={readOnly}
                  exportQuery={activeResult.query}
                  footerMode="none"
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
      ) : null}
    </div>
  );
}
