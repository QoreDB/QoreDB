// SPDX-License-Identifier: Apache-2.0

import { Driver } from '../connection/drivers';
import { buildDuckdbAlter } from './alter/duckdb';
import type { BuilderContext } from './alter/helpers';
import { buildMysqlAlter } from './alter/mysql';
import { buildPostgresAlter } from './alter/postgres';
import { buildSqliteAlter } from './alter/sqlite';
import { buildSqlServerAlter } from './alter/sqlserver';
import type { BuildResult } from './createTable';
import { buildQualifiedTableName } from './identifiers';
import type { AlterOp, TableDefinition } from './types';
import { warn } from './warnings';

export function buildAlterTableStatements(
  table: TableDefinition,
  ops: AlterOp[],
  driver: Driver
): BuildResult {
  if (ops.length === 0) return { statements: [], warnings: [] };
  const ctx: BuilderContext = {
    table,
    driver,
    fullName: buildQualifiedTableName(table.namespace, table.tableName, driver),
    warnings: [],
  };
  let statements: string[];
  switch (driver) {
    case Driver.Postgres:
    case Driver.Cockroachdb:
      statements = buildPostgresAlter(ctx, ops);
      break;
    case Driver.Mysql:
    case Driver.Mariadb:
      statements = buildMysqlAlter(ctx, ops);
      break;
    case Driver.Sqlite:
      statements = buildSqliteAlter(ctx, ops);
      break;
    case Driver.Duckdb:
      statements = buildDuckdbAlter(ctx, ops);
      break;
    case Driver.SqlServer:
      statements = buildSqlServerAlter(ctx, ops);
      break;
    default:
      ctx.warnings.push(warn('alter.driverUnsupported', { driver }));
      statements = [];
  }
  return { statements, warnings: ctx.warnings };
}
