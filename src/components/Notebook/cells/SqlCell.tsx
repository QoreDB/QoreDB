// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { SQLEditor } from '@/components/Editor/SQLEditor';
import type { Driver } from '@/lib/drivers';
import type { NotebookCell } from '@/lib/notebookTypes';
import type { Namespace } from '@/lib/tauri';
import { CellResultViewer } from '../results/CellResultViewer';

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

  const lineCount = useMemo(() => {
    return Math.max(3, Math.min(20, (cell.source.match(/\n/g)?.length ?? 0) + 1));
  }, [cell.source]);

  const editorHeight = lineCount * 20 + 16;

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
      {cell.lastResult && (
        <CellResultViewer result={cell.lastResult} maxRows={cell.config?.maxRows} />
      )}
    </div>
  );
}
