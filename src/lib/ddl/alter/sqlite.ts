// SPDX-License-Identifier: Apache-2.0

import { quoteIdentifier } from '../identifiers';
import type { AlterOp } from '../types';
import { warn } from '../warnings';
import {
  alterPrefix,
  type BuilderContext,
  buildIndexStmt,
  dropIndexStmt,
  newColumnSnippet,
} from './helpers';

export function buildSqliteAlter(ctx: BuilderContext, ops: AlterOp[]): string[] {
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
      case 'change_type':
      case 'set_nullable':
      case 'set_default':
        ctx.warnings.push(warn('sqlite.alterColumnUnsupported', { name: op.columnName }));
        break;
      case 'set_column_comment':
      case 'set_table_comment':
        ctx.warnings.push(warn('sqlite.commentsUnsupported'));
        break;
      case 'add_foreign_key':
      case 'drop_foreign_key':
        ctx.warnings.push(warn('sqlite.fkInPlaceUnsupported'));
        break;
      case 'add_check':
      case 'drop_check':
        ctx.warnings.push(warn('sqlite.checkInPlaceUnsupported'));
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
