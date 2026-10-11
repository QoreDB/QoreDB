// SPDX-License-Identifier: Apache-2.0

import { Binary } from 'lucide-react';
import {
  lazy,
  memo,
  type RefObject,
  Suspense,
  useCallback,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useTranslation } from 'react-i18next';
import { isBinaryType } from '@/lib/binaryUtils';
import { findViewerFor } from '@/lib/plugins';
import type { ForeignKey, Namespace, RelationFilter, Value } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { usePlugins } from '@/providers/PluginProvider';
import { ForeignKeyPeekTooltip } from './ForeignKeyPeekTooltip';
import type { PeekState } from './hooks/useForeignKeyPeek';
import { PluginCellRenderer } from './PluginCellRenderer';
import { formatCellPreview, type RowData } from './utils/dataGridUtils';

const BlobViewer = lazy(() =>
  import('./BlobViewer').then(module => ({ default: module.BlobViewer }))
);

export interface EditableDataCellProps {
  value: Value;
  columnId: string;
  rowId: string;
  row: RowData;
  dataType?: string;
  isEditing: boolean;
  editingValue: string;
  editInputRef: RefObject<HTMLInputElement | null>;
  onStartEdit: () => void;
  onCommitEdit: () => void;
  onCancelEdit: () => void;
  onEditValueChange: (value: string) => void;
  inlineEditAvailable: boolean;
  foreignKey?: ForeignKey;
  peekKey?: string;
  peekState?: PeekState;
  canPeek: boolean;
  onEnsurePeekLoaded: () => void;
  relationLabel: string;
  referencedNamespace: Namespace | null;
  hasMultipleRelations: boolean;
  onOpenRelatedTable?: (ns: Namespace, table: string, filter?: RelationFilter) => void;
}

export const EditableDataCell = memo(function EditableDataCell({
  value,
  columnId,
  dataType,
  isEditing,
  editingValue,
  editInputRef,
  onStartEdit,
  onCommitEdit,
  onCancelEdit,
  onEditValueChange,
  inlineEditAvailable,
  foreignKey,
  peekKey,
  peekState,
  canPeek,
  onEnsurePeekLoaded,
  relationLabel,
  referencedNamespace,
  hasMultipleRelations,
  onOpenRelatedTable,
}: EditableDataCellProps) {
  const { t } = useTranslation();
  const isBinary = Boolean(dataType && isBinaryType(dataType));
  const preview = useMemo(() => formatCellPreview(value, dataType), [value, dataType]);
  const formatted = preview.text;
  const isNull = value === null;
  const [blobViewerOpen, setBlobViewerOpen] = useState(false);
  const cellRef = useRef<HTMLButtonElement>(null);
  const restoreCellFocus = useCallback(() => {
    requestAnimationFrame(() => {
      const cell = cellRef.current;
      if (!cell?.isConnected) return;
      const active = cell.ownerDocument.activeElement;
      // Enter/Escape remove the input. Do not move focus back if the user or
      // a production confirmation dialog has already moved it elsewhere.
      if (active === cell.ownerDocument.body || (active && cell.contains(active))) {
        cell.focus({ preventScroll: true });
      }
    });
  }, []);
  const { contributions } = usePlugins();
  const pluginViewer = useMemo(
    () =>
      contributions.resultViewers.length === 0
        ? undefined
        : findViewerFor({ name: columnId, columnType: dataType }, contributions.resultViewers),
    [columnId, dataType, contributions.resultViewers]
  );

  const handleBlobClick = useCallback(() => {
    if (isBinary && typeof value === 'string' && value.length > 0) {
      setBlobViewerOpen(true);
    }
  }, [isBinary, value]);

  if (isBinary && !isNull && typeof value === 'string' && value.length > 0) {
    return (
      <>
        <div
          role="button"
          tabIndex={0}
          aria-label={t('blobViewer.title')}
          className="flex items-center gap-1.5 truncate cursor-pointer hover:text-accent transition-colors"
          onClick={handleBlobClick}
          onKeyDown={e => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              handleBlobClick();
            }
          }}
          title={t('blobViewer.title')}
        >
          <Binary className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          <span className="truncate text-muted-foreground italic text-xs">{formatted}</span>
        </div>
        {blobViewerOpen && (
          <Suspense fallback={null}>
            <BlobViewer
              open={blobViewerOpen}
              onOpenChange={setBlobViewerOpen}
              value={value}
              columnName={columnId}
              dataType={dataType ?? ''}
            />
          </Suspense>
        )}
      </>
    );
  }

  const cellContent = (
    <div className={cn('block', !isEditing && 'truncate', canPeek && 'group')}>
      {isEditing ? (
        <input
          ref={editInputRef}
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
          value={editingValue}
          onChange={event => onEditValueChange(event.target.value)}
          onBlur={() => void onCommitEdit()}
          onKeyDown={event => {
            if (event.nativeEvent.isComposing) return;
            if (event.key === 'Enter') {
              event.preventDefault();
              void onCommitEdit();
              restoreCellFocus();
            }
            if (event.key === 'Escape') {
              event.preventDefault();
              onCancelEdit();
              restoreCellFocus();
            }
          }}
          className="w-full bg-background border border-accent/50 rounded px-1.5 py-0.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-accent/40"
          aria-label={t('grid.editCell')}
        />
      ) : (
        <button
          ref={cellRef}
          type="button"
          tabIndex={inlineEditAvailable ? 0 : -1}
          aria-label={inlineEditAvailable ? `${t('grid.editCell')}: ${columnId}` : undefined}
          className={cn(
            'block w-full truncate text-left focus:outline-none focus:ring-2 focus:ring-accent/40',
            inlineEditAvailable ? 'cursor-text' : 'cursor-default'
          )}
          onClick={onStartEdit}
          onKeyDown={event => {
            if (inlineEditAvailable && event.key === 'F2') {
              event.preventDefault();
              onStartEdit();
            }
          }}
        >
          {pluginViewer && !isNull ? (
            <PluginCellRenderer viewer={pluginViewer} value={value} formatted={formatted} />
          ) : (
            <span
              className={cn(
                'truncate block',
                isNull && 'text-muted-foreground italic',
                canPeek && 'group-hover:text-foreground'
              )}
              title={preview.truncated ? t('grid.cellPreviewTruncated') : undefined}
            >
              {formatted}
              {preview.truncated && <span className="text-muted-foreground">…</span>}
            </span>
          )}
        </button>
      )}
    </div>
  );

  if (!canPeek || !foreignKey || !peekKey) {
    return cellContent;
  }

  return (
    <ForeignKeyPeekTooltip
      peekKey={peekKey}
      peekState={peekState}
      foreignKey={foreignKey}
      relationLabel={relationLabel}
      referencedNamespace={referencedNamespace}
      hasMultipleRelations={hasMultipleRelations}
      value={value}
      onOpenChange={open => {
        if (open) {
          onEnsurePeekLoaded();
        }
      }}
      onOpenRelatedTable={onOpenRelatedTable}
    >
      {cellContent}
    </ForeignKeyPeekTooltip>
  );
});
