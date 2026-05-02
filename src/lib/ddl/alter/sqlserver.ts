// SPDX-License-Identifier: Apache-2.0

import { quoteSqlString } from '../driverCapabilities';
import { quoteIdentifier } from '../identifiers';
import type { AlterOp, ColumnDef } from '../types';
import { warn } from '../warnings';
import {
  alterPrefix,
  type BuilderContext,
  buildIndexStmt,
  checkSnippet,
  dropIndexStmt,
  fkSnippet,
  newColumnSnippet,
} from './helpers';

export function buildSqlServerAlter(ctx: BuilderContext, ops: AlterOp[]): string[] {
  const stmts: string[] = [];
  const prefix = alterPrefix(ctx);
  for (const op of ops) {
    switch (op.kind) {
      case 'rename_table':
        stmts.push(
          `EXEC sp_rename ${quoteSqlString(ctx.fullName)}, ${quoteSqlString(op.newName)};`
        );
        break;
      case 'rename_column':
        stmts.push(
          `EXEC sp_rename ${quoteSqlString(`${ctx.fullName}.${op.from}`)}, ${quoteSqlString(op.to)}, 'COLUMN';`
        );
        break;
      case 'add_column':
        stmts.push(`${prefix} ADD ${newColumnSnippet(op.column, ctx)};`);
        break;
      case 'drop_column':
        stmts.push(`${prefix} DROP COLUMN ${quoteIdentifier(op.columnName, ctx.driver)};`);
        break;
      case 'change_type':
      case 'set_nullable': {
        const colDef = ctx.table.columns.find(c => c.name === op.columnName);
        if (!colDef) {
          ctx.warnings.push(warn('internal.columnNotFound', { name: op.columnName }));
          break;
        }
        const merged: ColumnDef =
          op.kind === 'change_type'
            ? {
                ...colDef,
                type: op.newType,
                length: op.length,
                precision: op.precision,
                scale: op.scale,
              }
            : { ...colDef, nullable: op.nullable };
        stmts.push(`${prefix} ALTER COLUMN ${newColumnSnippet(merged, ctx)};`);
        break;
      }
      case 'set_default':
        ctx.warnings.push(warn('sqlserver.defaultsManual'));
        break;
      case 'set_column_comment':
      case 'set_table_comment':
        ctx.warnings.push(warn('sqlserver.commentsManual'));
        break;
      case 'add_foreign_key':
        stmts.push(`${prefix} ADD ${fkSnippet(op.foreignKey, ctx)};`);
        break;
      case 'drop_foreign_key':
        stmts.push(`${prefix} DROP CONSTRAINT ${quoteIdentifier(op.name, ctx.driver)};`);
        break;
      case 'add_check':
        stmts.push(`${prefix} ADD ${checkSnippet(op.check, ctx)};`);
        break;
      case 'drop_check':
        stmts.push(`${prefix} DROP CONSTRAINT ${quoteIdentifier(op.name, ctx.driver)};`);
        break;
      case 'add_index': {
        const s = buildIndexStmt(op.index, ctx);
        if (s) stmts.push(s);
        break;
      }
      case 'drop_index':
        stmts.push(dropIndexStmt(op.name, ctx));
        break;
    }
  }
  return stmts;
}
