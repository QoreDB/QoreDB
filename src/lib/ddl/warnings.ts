// SPDX-License-Identifier: Apache-2.0

export type DdlWarningCode =
  | 'indexes.unsupported'
  | 'indexes.uniqueDowngraded'
  | 'indexes.methodIgnored'
  | 'indexes.partialUnsupported'
  | 'comments.columnUnsupported'
  | 'comments.tableUnsupported'
  | 'fk.unsupported'
  | 'check.unsupported'
  | 'sqlite.alterColumnUnsupported'
  | 'sqlite.commentsUnsupported'
  | 'sqlite.fkInPlaceUnsupported'
  | 'sqlite.checkInPlaceUnsupported'
  | 'duckdb.fkInPlaceLimited'
  | 'sqlserver.defaultsManual'
  | 'sqlserver.commentsManual'
  | 'internal.columnNotFound'
  | 'alter.driverUnsupported';

export type DdlWarningParams = Record<string, string | number | undefined>;

export interface DdlWarning {
  code: DdlWarningCode;
  params?: DdlWarningParams;
}

export function warn(code: DdlWarningCode, params?: DdlWarningParams): DdlWarning {
  return params ? { code, params } : { code };
}
