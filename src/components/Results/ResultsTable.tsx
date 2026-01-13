import { FixedSizeList as List } from 'react-window';
import { QueryResult } from '../../lib/tauri';
import './ResultsTable.css';

interface ResultsTableProps {
  result: QueryResult | null;
  height?: number;
}

const ROW_HEIGHT = 32;
const HEADER_HEIGHT = 36;

export function ResultsTable({ result, height = 400 }: ResultsTableProps) {
  if (!result || result.columns.length === 0) {
    if (result?.affected_rows !== undefined) {
      return (
        <div className="results-message">
          <span className="results-success">✓</span>
          {result.affected_rows} row(s) affected in {result.execution_time_ms}ms
        </div>
      );
    }
    return (
      <div className="results-empty">
        No results to display
      </div>
    );
  }

  const { columns, rows } = result;

  return (
    <div className="results-table" style={{ height }}>
      {/* Header */}
      <div className="results-header">
        {columns.map((col, i) => (
          <div key={i} className="results-header-cell" title={col.data_type}>
            {col.name}
          </div>
        ))}
      </div>

      {/* Virtualized rows */}
      <List
        height={height - HEADER_HEIGHT}
        itemCount={rows.length}
        itemSize={ROW_HEIGHT}
        width="100%"
      >
        {({ index, style }: { index: number; style: React.CSSProperties }) => (
          <div className="results-row" style={style}>
            {rows[index].values.map((value, colIndex) => (
              <div key={colIndex} className="results-cell">
                {formatValue(value)}
              </div>
            ))}
          </div>
        )}
      </List>

      {/* Footer */}
      <div className="results-footer">
        {rows.length} row(s) • {result.execution_time_ms}ms
      </div>
    </div>
  );
}

function formatValue(value: unknown): string {
  if (value === null) return 'NULL';
  if (value === undefined) return '';
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}
