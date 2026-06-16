// SPDX-License-Identifier: Apache-2.0

/**
 * Search query surface with a Console ⇄ SQL toggle.
 *
 * - Console mode: the "Dev Tools" editor (`METHOD /path` + JSON body).
 * - SQL mode: the standard {@link SQLEditor}, since Elasticsearch (`_sql`) and
 *   OpenSearch (`_plugins/_sql`) both speak a SQL dialect. The backend detects
 *   which one to run from the query text, so both feed the same execute path.
 *
 * The mode is local state — no plumbing through the query panel is required.
 */

import { type Ref, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { Driver } from '../../lib/connection/drivers';
import type { Namespace } from '../../lib/tauri';
import { SearchEditor } from './SearchEditor';
import { SQLEditor, type SQLEditorHandle } from './SQLEditor';

type SearchMode = 'console' | 'sql';

interface SearchConsoleProps {
  value: string;
  onChange: (value: string) => void;
  onExecute: () => void;
  onExecuteSelection: (selection: string) => void;
  onFormat: () => void;
  dialect: Driver;
  loading: boolean;
  sessionId?: string | null;
  connectionDatabase?: string;
  activeNamespace?: Namespace | null;
  sqlEditorRef?: Ref<SQLEditorHandle>;
}

export function SearchConsole({
  value,
  onChange,
  onExecute,
  onExecuteSelection,
  onFormat,
  dialect,
  loading,
  sessionId,
  connectionDatabase,
  activeNamespace,
  sqlEditorRef,
}: SearchConsoleProps) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<SearchMode>('console');

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-1 border-b border-border px-2 py-1">
        <ModeButton active={mode === 'console'} onClick={() => setMode('console')}>
          {t('search.modeConsole')}
        </ModeButton>
        <ModeButton active={mode === 'sql'} onClick={() => setMode('sql')}>
          {t('search.modeSql')}
        </ModeButton>
        <span className="ml-2 text-xs text-muted-foreground/70">
          {mode === 'console' ? t('search.modeConsoleHint') : t('search.modeSqlHint')}
        </span>
      </div>

      <div className="min-h-0 flex-1">
        {mode === 'sql' ? (
          <SQLEditor
            ref={sqlEditorRef}
            value={value}
            onChange={onChange}
            onExecute={onExecute}
            onExecuteSelection={onExecuteSelection}
            onFormat={onFormat}
            dialect={dialect}
            readOnly={loading}
            sessionId={sessionId}
            connectionDatabase={connectionDatabase}
            activeNamespace={activeNamespace}
            placeholder={t('search.sqlPlaceholder')}
          />
        ) : (
          <SearchEditor
            value={value}
            onChange={onChange}
            onExecute={onExecute}
            readOnly={loading}
            sessionId={sessionId}
            activeNamespace={activeNamespace}
          />
        )}
      </div>
    </div>
  );
}

function ModeButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={
        active
          ? 'rounded px-2 py-0.5 text-xs font-medium bg-accent text-accent-foreground'
          : 'rounded px-2 py-0.5 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent/10'
      }
    >
      {children}
    </button>
  );
}
