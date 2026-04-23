// SPDX-License-Identifier: Apache-2.0

import { Maximize2, Minimize2 } from 'lucide-react';
import type { Ref } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Tooltip } from '@/components/ui/tooltip';
import type { Driver } from '../../lib/drivers';
import type { Namespace } from '../../lib/tauri';
import { MongoEditor } from '../Editor/MongoEditor';
import { SQLEditor, type SQLEditorHandle } from '../Editor/SQLEditor';

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
  placeholder?: string;
  isExpanded?: boolean;
  onToggleExpand?: () => void;
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
  placeholder,
  isExpanded,
  onToggleExpand,
}: QueryPanelEditorProps) {
  const { t } = useTranslation();

  return (
    <div className="flex-1 min-h-0 border-b border-border relative group/editor">
      {isDocumentBased ? (
        <MongoEditor
          value={query}
          onChange={onQueryChange}
          onExecute={onExecute}
          readOnly={loading}
          sessionId={sessionId}
          activeNamespace={activeNamespace}
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
          placeholder={placeholder}
        />
      )}

      {onToggleExpand && (
        <Tooltip content={isExpanded ? t('query.collapseEditor') : t('query.expandEditor')}>
          <Button
            variant="ghost"
            size="icon"
            onClick={onToggleExpand}
            className="absolute top-1.5 right-1.5 h-6 w-6 text-muted-foreground/50 hover:text-foreground opacity-0 group-hover/editor:opacity-70 hover:!opacity-100 transition-opacity z-10"
          >
            {isExpanded ? <Minimize2 size={14} /> : <Maximize2 size={14} />}
          </Button>
        </Tooltip>
      )}
    </div>
  );
}
