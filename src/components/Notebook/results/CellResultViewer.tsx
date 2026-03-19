// SPDX-License-Identifier: Apache-2.0

import { DataGrid } from '@/components/Grid/DataGrid';
import type { CellResult } from '@/lib/notebookTypes';
import type { QueryResult } from '@/lib/tauri';
import { CellErrorViewer } from './CellErrorViewer';

interface CellResultViewerProps {
  result: CellResult;
  maxRows?: number;
}

export function CellResultViewer({ result, maxRows = 100 }: CellResultViewerProps) {
  if (result.type === 'error' && result.error) {
    return <CellErrorViewer error={result.error} />;
  }

  if (result.type === 'message' && result.message) {
    return (
      <div className="mt-2 px-3 py-2 bg-muted/50 border border-border rounded-md text-sm text-muted-foreground">
        {result.message}
      </div>
    );
  }

  if (result.type === 'table' && result.columns && result.rows) {
    const queryResult: QueryResult = {
      columns: result.columns,
      rows: result.rows.slice(0, maxRows),
      affected_rows: result.affectedRows,
      execution_time_ms: 0,
    };

    return (
      <div
        className="mt-2 border border-border/50 rounded-md overflow-hidden"
        style={{ maxHeight: 320 }}
      >
        <DataGrid result={queryResult} readOnly environment="development" />
      </div>
    );
  }

  return null;
}
