// SPDX-License-Identifier: Apache-2.0

export { buildAlterTableSQL, type DiffOptions, diffTableDefinitions } from './alterTable';
export { buildAlterTableStatements } from './alterTableBuilders';
export {
  type BuildResult,
  buildColumnSQL,
  buildCreateTableSQL,
  buildCreateTableStatements,
} from './createTable';
export {
  type DdlCapabilities,
  getDdlCapabilities,
  type IndexMethodPlacement,
  quoteSqlString,
} from './driverCapabilities';
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
export type { DdlWarning, DdlWarningCode, DdlWarningParams } from './warnings';
