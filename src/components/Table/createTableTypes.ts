// SPDX-License-Identifier: Apache-2.0

import type { CheckConstraintDef, ColumnDef, ForeignKeyDef, IndexDef } from '@/lib/ddl';

export interface IdentifiedItem {
  _id: string;
}

export type EditableColumn = ColumnDef & IdentifiedItem & { _originalName?: string };
export type EditableForeignKey = ForeignKeyDef & IdentifiedItem;
export type EditableIndex = IndexDef & IdentifiedItem;
export type EditableCheck = CheckConstraintDef & IdentifiedItem;

export type CreateTableSection = 'columns' | 'foreignKeys' | 'indexes' | 'checks' | 'sql';
