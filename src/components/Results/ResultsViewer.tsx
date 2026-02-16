/**
 * ResultsViewer - Universal data display wrapper
 *
 * Routes to the appropriate viewer based on driver capabilities:
 * - DocumentResults for document-based databases (MongoDB, etc.)
 * - DataGrid for relational databases (PostgreSQL, MySQL, etc.)
 */
import { isDocumentDatabase } from '@/lib/driverCapabilities';
import { DataGrid } from '../Grid/DataGrid';
import { DocumentResults } from './DocumentResults';
import { Driver } from '@/lib/drivers';
import { QueryResult, Value, Environment, Namespace, TableSchema, SortDirection } from '@/lib/tauri';
import { SandboxChange, SandboxDeleteDisplay } from '@/lib/sandboxTypes';

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

  serverSideTotalRows?: number;
  serverSidePage?: number;
  serverSidePageSize?: number;
  onServerPageChange?: (page: number) => void;
  onServerPageSizeChange?: (pageSize: number) => void;
  serverSortColumn?: string;
  serverSortDirection?: SortDirection;
  onServerSortChange?: (column?: string, direction?: SortDirection) => void;
  serverSearchTerm?: string;
  onServerSearchChange?: (search: string) => void;

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

export function ResultsViewer({
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
  serverSideTotalRows,
  serverSidePage,
  serverSidePageSize,
  onServerPageChange,
  onServerPageSizeChange,
  serverSortColumn,
  serverSortDirection,
  onServerSortChange,
  serverSearchTerm,
  onServerSearchChange,
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

  if (isDocument) {
    return (
      <DocumentResults
        result={result!}
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
        serverSideTotalRows={serverSideTotalRows}
        serverSidePage={serverSidePage}
        serverSidePageSize={serverSidePageSize}
        onServerPageChange={onServerPageChange}
        onServerPageSizeChange={onServerPageSizeChange}
      />
    );
  }

  return (
    <DataGrid
      result={result}
      sessionId={sessionId}
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
      serverSideTotalRows={serverSideTotalRows}
      serverSidePage={serverSidePage}
      serverSidePageSize={serverSidePageSize}
      onServerPageChange={onServerPageChange}
      onServerPageSizeChange={onServerPageSizeChange}
      serverSortColumn={serverSortColumn}
      serverSortDirection={serverSortDirection}
      onServerSortChange={onServerSortChange}
      serverSearchTerm={serverSearchTerm}
      onServerSearchChange={onServerSearchChange}
      sandboxMode={sandboxMode}
      pendingChanges={pendingChanges}
      sandboxDeleteDisplay={sandboxDeleteDisplay}
      onSandboxUpdate={onSandboxUpdate}
      onSandboxDelete={onSandboxDelete}
      exportQuery={exportQuery}
    />
  );
}
