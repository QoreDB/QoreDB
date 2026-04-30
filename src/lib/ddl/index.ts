// SPDX-License-Identifier: Apache-2.0

export { buildAlterTableSQL, diffTableDefinitions } from './alterTable';
export { buildColumnSQL, buildCreateTableSQL } from './createTable';
export { buildQualifiedTableName, quoteIdentifier } from './identifiers';
export { buildDropTableSQL, buildTruncateTableSQL } from './tableOperations';
export { COLUMN_TYPES, getColumnTypes } from './typeDefinitions';
export type {
  AlterOp,
  CheckConstraintDef,
  ColumnCategory,
  ColumnDef,
  ColumnType,
  ForeignKeyDef,
  IndexDef,
  NamespaceLike,
  ReferentialAction,
  TableDefinition,
} from './types';
