// SPDX-License-Identifier: Apache-2.0

/**
 * Editable data cell component for DataGrid
 * Handles cell display, inline editing, and foreign key peek tooltips
 */

import type { RefObject } from 'react';
import { useTranslation } from 'react-i18next';
import type { ForeignKey, Namespace, RelationFilter, Value } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { ForeignKeyPeekTooltip } from './ForeignKeyPeekTooltip';
import type { PeekState } from './hooks/useForeignKeyPeek';
import { formatValue, type RowData } from './utils/dataGridUtils';

export interface EditableDataCellProps {
  value: Value;
  columnId: string;
  rowId: string;
  row: RowData;
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
  const formatted = formatValue(value);
  const isNull = value === null;

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
