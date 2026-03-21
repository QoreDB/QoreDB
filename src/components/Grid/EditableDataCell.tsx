// SPDX-License-Identifier: Apache-2.0

/**
 * Editable data cell component for DataGrid
 * Handles cell display, inline editing, and foreign key peek tooltips
 */

import { Binary } from 'lucide-react';
import { type RefObject, useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { isBinaryType } from '@/lib/binaryUtils';
import type { ForeignKey, Namespace, RelationFilter, Value } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { BlobViewer } from './BlobViewer';
import { ForeignKeyPeekTooltip } from './ForeignKeyPeekTooltip';
import type { PeekState } from './hooks/useForeignKeyPeek';
import { formatValue, type RowData } from './utils/dataGridUtils';

export interface EditableDataCellProps {
  value: Value;
  columnId: string;
  rowId: string;
  row: RowData;
  /** Database column type (e.g., "bytea", "blob", "varchar") */
  dataType?: string;
  // Editing props
  isEditing: boolean;
  editingValue: string;
  editInputRef: RefObject<HTMLInputElement | null>;
  onStartEdit: () => void;
  onCommitEdit: () => void;
  onCancelEdit: () => void;
  onEditValueChange: (value: string) => void;
  inlineEditAvailable: boolean;
  // Foreign key peek props
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

export function EditableDataCell({
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
  const formatted = formatValue(value, dataType);
  const isNull = value === null;
  const [blobViewerOpen, setBlobViewerOpen] = useState(false);

  const handleBlobClick = useCallback(() => {
    if (isBinary && typeof value === 'string' && value.length > 0) {
      setBlobViewerOpen(true);
    }
  }, [isBinary, value]);

  // Binary cell: special rendering with icon + click to open viewer
  if (isBinary && !isNull && typeof value === 'string' && value.length > 0) {
    return (
      <>
        <div
          className="flex items-center gap-1.5 truncate cursor-pointer hover:text-accent transition-colors"
          onClick={handleBlobClick}
          title={t('blobViewer.title')}
        >
          <Binary className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          <span className="truncate text-muted-foreground italic text-xs">{formatted}</span>
        </div>
        <BlobViewer
          open={blobViewerOpen}
          onOpenChange={setBlobViewerOpen}
          value={value}
          columnName={columnId}
          dataType={dataType ?? ''}
        />
      </>
    );
  }

  const cellContent = (
    <div
      className={cn(
        'block',
        !isEditing && 'truncate',
        !isEditing && inlineEditAvailable && 'cursor-text',
        canPeek && 'group'
      )}
      onClick={onStartEdit}
      onDoubleClick={onStartEdit}
    >
      {isEditing ? (
        <input
          ref={editInputRef}
          value={editingValue}
          onChange={event => onEditValueChange(event.target.value)}
          onBlur={() => void onCommitEdit()}
          onKeyDown={event => {
            if (event.key === 'Enter') {
              event.preventDefault();
              void onCommitEdit();
            }
            if (event.key === 'Escape') {
              event.preventDefault();
              onCancelEdit();
            }
          }}
          className="w-full bg-background border border-accent/50 rounded px-1.5 py-0.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-accent/40"
          aria-label={t('grid.editCell')}
        />
      ) : (
        <span
          className={cn(
            'truncate block',
            isNull && 'text-muted-foreground italic',
            canPeek && 'group-hover:text-foreground'
          )}
        >
          {formatted}
        </span>
      )}
    </div>
  );

  // If no foreign key peek, just return the cell content
  if (!canPeek || !foreignKey || !peekKey) {
    return cellContent;
  }

  // Wrap with foreign key peek tooltip
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
}
