// SPDX-License-Identifier: Apache-2.0

import { ChevronDown, ChevronRight } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { SQLEditor } from '@/components/Editor/SQLEditor';
import type { Driver } from '@/lib/connection/drivers';
import type { NotebookCell } from '@/lib/notebook/notebookTypes';
import type { Namespace } from '@/lib/tauri';
import { CellResultViewer } from '../results/CellResultViewer';

const DEFAULT_VISIBLE_ROWS = 5;

interface SqlCellProps {
  cell: NotebookCell;
  dialect?: Driver;
  sessionId?: string | null;
  connectionDatabase?: string;
  namespace?: Namespace | null;
  onSourceChange: (source: string) => void;
  onExecute: () => void;
}

export function SqlCell({
  cell,
  dialect,
  sessionId,
  connectionDatabase,
  namespace,
  onSourceChange,
  onExecute,
}: SqlCellProps) {
  const { t } = useTranslation();
  const [showResults, setShowResults] = useState(true);

  const lineCount = useMemo(() => {
    return Math.max(3, Math.min(20, (cell.source.match(/\n/g)?.length ?? 0) + 1));
  }, [cell.source]);

  const editorHeight = lineCount * 20 + 16;

  const hasResult = !!cell.lastResult;
  const rowCount = cell.lastResult?.totalRows;

  return (
    <div>
      <div
        className="rounded-md overflow-hidden bg-muted/20"
        style={{ height: editorHeight, minHeight: 76, maxHeight: 416 }}
      >
        <SQLEditor
          value={cell.source}
          onChange={onSourceChange}
          onExecute={onExecute}
          dialect={dialect}
          sessionId={sessionId}
          connectionDatabase={connectionDatabase}
          activeNamespace={namespace}
          placeholder={t('notebook.sqlPlaceholder')}
        />
      </div>
      {hasResult && (
        <div className="mt-1.5">
          <button
            type="button"
            onClick={() => setShowResults(prev => !prev)}
            className="flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground transition-colors py-0.5"
          >
            {showResults ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
            <span>{showResults ? t('notebook.hideResults') : t('notebook.showResults')}</span>
            {rowCount !== undefined && (
              <span className="text-muted-foreground/70">
                ({t('notebook.rowCount', { count: rowCount })})
              </span>
            )}
          </button>
          {showResults && cell.lastResult && (
            <CellResultViewer
              result={cell.lastResult}
              maxRows={cell.config?.maxRows ?? DEFAULT_VISIBLE_ROWS}
            />
          )}
        </div>
      )}
    </div>
  );
}
