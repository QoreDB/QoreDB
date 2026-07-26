// SPDX-License-Identifier: Apache-2.0

import { memo } from 'react';
import { isDocumentDatabase } from '@/lib/connection/driverCapabilities';
import type { Driver } from '@/lib/connection/drivers';
import type { SandboxChange, SandboxDeleteDisplay } from '@/lib/sandbox/sandboxTypes';
import type {
  CancelSupport,
  ColumnFilter,
  Environment,
  Namespace,
  OrderingGuarantee,
  QueryResult,
  SortDirection,
  TableSchema,
  TotalRowsSource,
  Value,
} from '@/lib/tauri';
import { DataGrid } from '../Grid/DataGrid';
import type { SearchScopeState } from '../Grid/SearchScopeControl';
import { ErrorBoundary } from '../ui/error-boundary';
import { DocumentResults } from './DocumentResults';

interface ResultsViewerProps {
  result: QueryResult | null;
  sessionId?: string;
  driver: Driver;
  environment?: Environment;
  readOnly?: boolean;
  connectionName?: string;
  connectionDatabase?: string;
  onRowsDeleted?: () => void;
  namespace?: Namespace;
  tableName?: string;
  tableSchema?: TableSchema | null;
  primaryKey?: string[];
  mutationsSupported?: boolean;
  initialFilter?: string;
  onRowsUpdated?: () => void;
  onOpenRelatedTable?: (namespace: Namespace, tableName: string) => void;
  onRowClick?: (row: Record<string, Value>) => void;
  infiniteScrollTotalRows?: number | null;
  infiniteScrollTotalRowsSource?: TotalRowsSource | null;
  infiniteScrollTotalRowsAsOf?: number | null;
  infiniteScrollLoadedRows?: number;
  infiniteScrollIsFetchingMore?: boolean;
  infiniteScrollIsCountingTotal?: boolean;
  infiniteScrollIsComplete?: boolean;
  infiniteScrollWindowExhausted?: boolean;
  infiniteScrollOrderingGuarantee?: OrderingGuarantee;
  onFetchMore?: () => void;
  onCalculateExactTotal?: () => void;
  onCancelExactTotal?: () => void;
  infiniteScrollCancelSupport?: CancelSupport;
  searchScope?: SearchScopeState;
  onCreateIndex?: (column: string) => void;
  serverSortColumn?: string;
  serverSortDirection?: SortDirection;
  onServerSortChange?: (column?: string, direction?: SortDirection) => void;
  serverSearchTerm?: string;
  onServerSearchChange?: (search: string) => void;
  onServerColumnFiltersChange?: (filters: ColumnFilter[]) => void;
  sandboxMode?: boolean;
  pendingChanges?: SandboxChange[];
  sandboxDeleteDisplay?: SandboxDeleteDisplay;
  onSandboxUpdate?: (
    primaryKey: Record<string, Value>,
    oldValues: Record<string, Value>,
    newValues: Record<string, Value>
  ) => void;
  onSandboxDelete?: (primaryKey: Record<string, Value>, oldValues: Record<string, Value>) => void;

  database?: string;
  collection?: string;
  onEditDocument?: (doc: Record<string, unknown>, idValue?: Value) => void;
  exportQuery?: string;
  exportNamespace?: Namespace;
}

export const ResultsViewer = memo(function ResultsViewer({
  result,
  sessionId,
  driver,
  environment = 'development',
  readOnly = false,
  connectionName,
  connectionDatabase,
  onRowsDeleted,
  namespace,
  tableName,
  tableSchema,
  primaryKey,
  mutationsSupported,
  initialFilter,
  onRowsUpdated,
  onOpenRelatedTable,
  onRowClick,
  infiniteScrollTotalRows,
  infiniteScrollTotalRowsSource,
  infiniteScrollTotalRowsAsOf,
  infiniteScrollLoadedRows,
  infiniteScrollIsFetchingMore,
  infiniteScrollIsCountingTotal,
  infiniteScrollIsComplete,
  infiniteScrollWindowExhausted,
  infiniteScrollOrderingGuarantee,
  onFetchMore,
  onCalculateExactTotal,
  onCancelExactTotal,
  infiniteScrollCancelSupport,
  searchScope,
  onCreateIndex,
  serverSortColumn,
  serverSortDirection,
  onServerSortChange,
  serverSearchTerm,
  onServerSearchChange,
  onServerColumnFiltersChange,
  sandboxMode,
  pendingChanges,
  sandboxDeleteDisplay,
  onSandboxUpdate,
  onSandboxDelete,
  database,
  collection,
  onEditDocument,
  exportQuery,
  exportNamespace,
}: ResultsViewerProps) {
  const isDocument = isDocumentDatabase(driver);
  const safeResult: QueryResult = result ?? {
    columns: [],
    rows: [],
    execution_time_ms: 0,
  };

  if (isDocument) {
    return (
      <DocumentResults
        result={safeResult}
        sessionId={sessionId}
        database={database}
        collection={collection}
        environment={environment}
        readOnly={readOnly}
        connectionName={connectionName}
        connectionDatabase={connectionDatabase}
        onEditDocument={onEditDocument}
        onRowsDeleted={onRowsDeleted}
        exportQuery={exportQuery}
        exportNamespace={exportNamespace}
        infiniteScrollTotalRows={infiniteScrollTotalRows}
        infiniteScrollTotalRowsSource={infiniteScrollTotalRowsSource}
        infiniteScrollTotalRowsAsOf={infiniteScrollTotalRowsAsOf}
        infiniteScrollLoadedRows={infiniteScrollLoadedRows}
        infiniteScrollIsFetchingMore={infiniteScrollIsFetchingMore}
        infiniteScrollIsCountingTotal={infiniteScrollIsCountingTotal}
        infiniteScrollIsComplete={infiniteScrollIsComplete}
        infiniteScrollWindowExhausted={infiniteScrollWindowExhausted}
        infiniteScrollOrderingGuarantee={infiniteScrollOrderingGuarantee}
        onFetchMore={onFetchMore}
        onCalculateExactTotal={onCalculateExactTotal}
        onCancelExactTotal={onCancelExactTotal}
        infiniteScrollCancelSupport={infiniteScrollCancelSupport}
        serverSearchTerm={serverSearchTerm}
        onServerSearchChange={onServerSearchChange}
      />
    );
  }

  return (
    <ErrorBoundary>
      <DataGrid
        result={safeResult}
        sessionId={sessionId}
        driver={driver}
        namespace={namespace}
        tableName={tableName}
        tableSchema={tableSchema}
        primaryKey={primaryKey}
        environment={environment}
        readOnly={readOnly}
        mutationsSupported={mutationsSupported}
        connectionName={connectionName}
        connectionDatabase={connectionDatabase}
        initialFilter={initialFilter}
        onRowsDeleted={onRowsDeleted}
        onRowsUpdated={onRowsUpdated}
        onOpenRelatedTable={onOpenRelatedTable}
        onRowClick={onRowClick}
        infiniteScrollTotalRows={infiniteScrollTotalRows}
        infiniteScrollLoadedRows={infiniteScrollLoadedRows}
        infiniteScrollIsFetchingMore={infiniteScrollIsFetchingMore}
        infiniteScrollIsCountingTotal={infiniteScrollIsCountingTotal}
        infiniteScrollIsComplete={infiniteScrollIsComplete}
        infiniteScrollWindowExhausted={infiniteScrollWindowExhausted}
        infiniteScrollOrderingGuarantee={infiniteScrollOrderingGuarantee}
        onFetchMore={onFetchMore}
        onCalculateExactTotal={onCalculateExactTotal}
        onCancelExactTotal={onCancelExactTotal}
        infiniteScrollCancelSupport={infiniteScrollCancelSupport}
        searchScope={searchScope}
        onCreateIndex={onCreateIndex}
        serverSortColumn={serverSortColumn}
        serverSortDirection={serverSortDirection}
        onServerSortChange={onServerSortChange}
        serverSearchTerm={serverSearchTerm}
        onServerSearchChange={onServerSearchChange}
        onServerColumnFiltersChange={onServerColumnFiltersChange}
        sandboxMode={sandboxMode}
        pendingChanges={pendingChanges}
        sandboxDeleteDisplay={sandboxDeleteDisplay}
        onSandboxUpdate={onSandboxUpdate}
        onSandboxDelete={onSandboxDelete}
        exportQuery={exportQuery}
      />
    </ErrorBoundary>
  );
});
