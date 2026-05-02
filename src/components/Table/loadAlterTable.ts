// SPDX-License-Identifier: Apache-2.0

import type { ColumnDef, ForeignKeyDef, IndexDef, NamespaceLike, TableDefinition } from '@/lib/ddl';
import type { TableSchema } from '@/lib/tauri';
import type { EditableColumn, EditableForeignKey, EditableIndex } from './createTableTypes';

interface IdGen {
  next: (prefix: string) => string;
}

export function tableSchemaToColumns(schema: TableSchema, idGen: IdGen): EditableColumn[] {
  return schema.columns.map(col => ({
    _id: idGen.next('column'),
    _originalName: col.name,
    name: col.name,
    type: col.data_type,
    nullable: col.nullable,
    isPrimaryKey: col.is_primary_key,
    isUnique: false,
    defaultValue: col.default_value,
  }));
}

export function tableSchemaToForeignKeys(schema: TableSchema, idGen: IdGen): EditableForeignKey[] {
  return schema.foreign_keys
    .filter(fk => !fk.is_virtual)
    .map(fk => ({
      _id: idGen.next('fk'),
      name: fk.constraint_name,
      columns: [fk.column],
      refSchema: fk.referenced_schema,
      refTable: fk.referenced_table,
      refColumns: [fk.referenced_column],
    }));
}

export function tableSchemaToIndexes(schema: TableSchema, idGen: IdGen): EditableIndex[] {
  return schema.indexes
    .filter(idx => !idx.is_primary)
    .map(idx => ({
      _id: idGen.next('idx'),
      name: idx.name,
      columns: idx.columns,
      unique: idx.is_unique,
      method: idx.index_type ?? undefined,
    }));
}

export function buildAlterSnapshot(
  namespace: NamespaceLike,
  tableName: string,
  columns: EditableColumn[],
  foreignKeys: EditableForeignKey[],
  indexes: EditableIndex[]
): TableDefinition {
  return {
    namespace,
    tableName,
    columns: columns.map(c => {
      const { _id, _originalName, ...rest } = c;
      void _id;
      void _originalName;
      return rest as ColumnDef;
    }),
    foreignKeys: foreignKeys.map(fk => {
      const { _id, ...rest } = fk;
      void _id;
      return rest as ForeignKeyDef;
    }),
    indexes: indexes.map(idx => {
      const { _id, ...rest } = idx;
      void _id;
      return rest as IndexDef;
    }),
  };
}
