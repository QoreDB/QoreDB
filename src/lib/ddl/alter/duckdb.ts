// SPDX-License-Identifier: Apache-2.0

import { quoteSqlString } from '../driverCapabilities';
import { quoteIdentifier } from '../identifiers';
import type { AlterOp } from '../types';
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

export function buildDuckdbAlter(ctx: BuilderContext, ops: AlterOp[]): string[] {
  const stmts: string[] = [];
  const prefix = alterPrefix(ctx);
  for (const op of ops) {
    switch (op.kind) {
      case 'rename_table':
        stmts.push(`${prefix} RENAME TO ${quoteIdentifier(op.newName, ctx.driver)};`);
        break;
      case 'rename_column':
        stmts.push(
          `${prefix} RENAME COLUMN ${quoteIdentifier(op.from, ctx.driver)} TO ${quoteIdentifier(op.to, ctx.driver)};`
        );
        break;
      case 'add_column':
        stmts.push(`${prefix} ADD COLUMN ${newColumnSnippet(op.column, ctx)};`);
        break;
      case 'drop_column':
        stmts.push(`${prefix} DROP COLUMN ${quoteIdentifier(op.columnName, ctx.driver)};`);
        break;
      case 'change_type': {
        const id = quoteIdentifier(op.columnName, ctx.driver);
        let typeSql = op.newType;
        if (op.length) typeSql = `${op.newType}(${op.length})`;
        else if (op.precision)
          typeSql = `${op.newType}(${op.precision}${op.scale ? `, ${op.scale}` : ''})`;
        stmts.push(`${prefix} ALTER COLUMN ${id} TYPE ${typeSql};`);
        break;
      }
      case 'set_nullable': {
        const id = quoteIdentifier(op.columnName, ctx.driver);
        stmts.push(
          op.nullable
            ? `${prefix} ALTER COLUMN ${id} DROP NOT NULL;`
            : `${prefix} ALTER COLUMN ${id} SET NOT NULL;`
        );
        break;
      }
      case 'set_default': {
        const id = quoteIdentifier(op.columnName, ctx.driver);
        stmts.push(
          op.defaultValue !== undefined && op.defaultValue !== ''
            ? `${prefix} ALTER COLUMN ${id} SET DEFAULT ${op.defaultValue};`
            : `${prefix} ALTER COLUMN ${id} DROP DEFAULT;`
        );
        break;
      }
      case 'set_column_comment':
        stmts.push(
          `COMMENT ON COLUMN ${ctx.fullName}.${quoteIdentifier(op.columnName, ctx.driver)} IS ${quoteSqlString(op.comment)};`
        );
        break;
      case 'set_table_comment':
        stmts.push(`COMMENT ON TABLE ${ctx.fullName} IS ${quoteSqlString(op.comment)};`);
        break;
      case 'add_foreign_key':
        ctx.warnings.push(warn('duckdb.fkInPlaceLimited'));
        stmts.push(`${prefix} ADD ${fkSnippet(op.foreignKey, ctx)};`);
        break;
      case 'drop_foreign_key':
        ctx.warnings.push(warn('duckdb.fkInPlaceLimited'));
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
