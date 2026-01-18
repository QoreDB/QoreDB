import { SQLEditor } from '../Editor/SQLEditor';
import { MongoEditor } from '../Editor/MongoEditor';
import { Driver } from '../../lib/drivers';

interface QueryPanelEditorProps {
  isMongo: boolean;
  query: string;
  loading: boolean;
  dialect: Driver;
  onQueryChange: (value: string) => void;
  onExecute: () => void;
  onExecuteSelection: (selection: string) => void;
}

export function QueryPanelEditor({
  isMongo,
  query,
  loading,
  dialect,
  onQueryChange,
  onExecute,
  onExecuteSelection,
}: QueryPanelEditorProps) {
  return (
    <div className="flex-1 min-h-50 border-b border-border relative">
      {isMongo ? (
        <MongoEditor value={query} onChange={onQueryChange} onExecute={onExecute} readOnly={loading} />
      ) : (
        <SQLEditor
          value={query}
          onChange={onQueryChange}
          onExecute={onExecute}
          onExecuteSelection={onExecuteSelection}
          dialect={dialect}
          readOnly={loading}
        />
      )}
    </div>
  );
}
