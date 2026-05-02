// SPDX-License-Identifier: Apache-2.0

export type ColumnCategory =
  | 'integer'
  | 'float'
  | 'string'
  | 'text'
  | 'date'
  | 'binary'
  | 'json'
  | 'boolean'
  | 'other';

export interface ColumnType {
  name: string;
  category: ColumnCategory;
  hasLength?: boolean;
  hasPrecision?: boolean;
  isAutoIncrement?: boolean;
}

export interface ColumnDef {
  name: string;
  type: string;
  length?: number;
  precision?: number;
  scale?: number;
  nullable: boolean;
  defaultValue?: string;
  isPrimaryKey: boolean;
  isUnique: boolean;
  isAutoIncrement?: boolean;
  comment?: string;
}

export interface NamespaceLike {
  database: string;
  schema?: string | null;
}

export type ReferentialAction = 'CASCADE' | 'SET NULL' | 'SET DEFAULT' | 'RESTRICT' | 'NO ACTION';

export interface ForeignKeyDef {
  name?: string;
  columns: string[];
  refSchema?: string | null;
  refTable: string;
  refColumns: string[];
  onDelete?: ReferentialAction;
  onUpdate?: ReferentialAction;
}

export interface IndexDef {
  name: string;
  columns: string[];
  unique?: boolean;
  method?: string;
  where?: string;
}

export interface CheckConstraintDef {
  name?: string;
  expression: string;
}

export interface TableDefinition {
  namespace: NamespaceLike;
  tableName: string;
  columns: ColumnDef[];
  foreignKeys?: ForeignKeyDef[];
  indexes?: IndexDef[];
  checks?: CheckConstraintDef[];
  comment?: string;
}

export type AlterOp =
  | { kind: 'add_column'; column: ColumnDef }
  | { kind: 'drop_column'; columnName: string }
  | { kind: 'rename_column'; from: string; to: string }
  | {
      kind: 'change_type';
      columnName: string;
      newType: string;
      length?: number;
      precision?: number;
      scale?: number;
    }
  | { kind: 'set_nullable'; columnName: string; nullable: boolean }
  | { kind: 'set_default'; columnName: string; defaultValue?: string }
  | { kind: 'set_column_comment'; columnName: string; comment: string }
  | { kind: 'add_foreign_key'; foreignKey: ForeignKeyDef }
  | { kind: 'drop_foreign_key'; name: string }
  | { kind: 'add_index'; index: IndexDef }
  | { kind: 'drop_index'; name: string }
  | { kind: 'add_check'; check: CheckConstraintDef }
  | { kind: 'drop_check'; name: string }
  | { kind: 'rename_table'; newName: string }
  | { kind: 'set_table_comment'; comment: string };
