import { useTranslation } from 'react-i18next';
import { AlertCircle } from 'lucide-react';
import { DataGrid } from '../Grid/DataGrid';
import { DocumentResults } from '../Results/DocumentResults';
import { Environment, QueryResult, Value } from '../../lib/tauri';
import { getCollectionFromQuery } from './queryPanelUtils';

interface QueryPanelResultsProps {
  error: string | null;
  result: QueryResult | null;
  isMongo: boolean;
  sessionId: string | null;
  connectionName?: string;
  connectionDatabase?: string;
  environment: Environment;
  readOnly: boolean;
  query: string;
  onRowsDeleted: () => void;
  onEditDocument: (doc: Record<string, unknown>, idValue?: Value) => void;
}

export function QueryPanelResults({
  error,
  result,
  isMongo,
  sessionId,
  connectionName,
  connectionDatabase,
  environment,
  readOnly,
  query,
  onRowsDeleted,
  onEditDocument,
}: QueryPanelResultsProps) {
  const { t } = useTranslation();
  const collection = getCollectionFromQuery(query);

  return (
    <div className="flex-1 flex flex-col min-h-0 bg-background overflow-hidden relative">
      {error ? (
        <div className="p-4 m-4 rounded-md bg-error/10 border border-error/20 text-error flex items-start gap-3">
          <AlertCircle className="mt-0.5 shrink-0" size={18} />
          <pre className="text-sm font-mono whitespace-pre-wrap break-all">{error}</pre>
        </div>
      ) : result ? (
        isMongo ? (
          <div className="flex-1 overflow-hidden p-2 flex flex-col h-full">
            <DocumentResults
              result={result}
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
          <div className="flex-1 overflow-hidden p-2 flex flex-col h-full">
            <DataGrid
              result={result}
              sessionId={sessionId || undefined}
              connectionName={connectionName}
              connectionDatabase={connectionDatabase}
              environment={environment}
              readOnly={readOnly}
            />
          </div>
        )
      ) : (
        <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
          {t('query.noResults')}
        </div>
      )}
    </div>
  );
}
