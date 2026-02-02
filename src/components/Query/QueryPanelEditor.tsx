import { Ref } from 'react';
import { SQLEditor, SQLEditorHandle } from '../Editor/SQLEditor';
import { MongoEditor } from '../Editor/MongoEditor';
import { Driver } from '../../lib/drivers';
import { Namespace } from '../../lib/tauri';

interface QueryPanelEditorProps {
  isDocumentBased: boolean;
  query: string;
  loading: boolean;
  dialect: Driver;
  sessionId?: string | null;
  connectionDatabase?: string;
  activeNamespace?: Namespace | null;
  onQueryChange: (value: string) => void;
  onExecute: () => void;
  onExecuteSelection: (selection: string) => void;
  onFormat: () => void;
  sqlEditorRef?: Ref<SQLEditorHandle>;
}

export function QueryPanelEditor({
  isDocumentBased,
  query,
  loading,
  dialect,
  sessionId,
  connectionDatabase,
  activeNamespace,
  onQueryChange,
  onExecute,
  onExecuteSelection,
  onFormat,
  sqlEditorRef,
}: QueryPanelEditorProps) {
  return (
    <div className="flex-1 min-h-50 border-b border-border relative">
      {isDocumentBased ? (
        <MongoEditor
          value={query}
          onChange={onQueryChange}
          onExecute={onExecute}
          readOnly={loading}
        />
      ) : (
        <SQLEditor
          ref={sqlEditorRef}
          value={query}
          onChange={onQueryChange}
          onExecute={onExecute}
          onExecuteSelection={onExecuteSelection}
          onFormat={onFormat}
          dialect={dialect}
          readOnly={loading}
          sessionId={sessionId}
          connectionDatabase={connectionDatabase}
          activeNamespace={activeNamespace}
        />
      )}
    </div>
  );
}
