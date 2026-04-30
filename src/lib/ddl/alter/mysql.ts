// SPDX-License-Identifier: Apache-2.0

import { Driver } from '../../drivers';
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

export function buildMysqlAlter(ctx: BuilderContext, ops: AlterOp[]): string[] {
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
        const colDef = ctx.table.columns.find(c => c.name === op.columnName);
        if (!colDef) {
          ctx.warnings.push(warn('internal.columnNotFound', { name: op.columnName }));
          break;
        }
        const merged: ColumnDef = {
          ...colDef,
          type: op.newType,
          length: op.length,
          precision: op.precision,
          scale: op.scale,
        };
        stmts.push(`${prefix} MODIFY COLUMN ${newColumnSnippet(merged, ctx)};`);
        break;
      }
      case 'set_nullable': {
        const colDef = ctx.table.columns.find(c => c.name === op.columnName);
        if (!colDef) {
          ctx.warnings.push(warn('internal.columnNotFound', { name: op.columnName }));
          break;
        }
        const merged: ColumnDef = { ...colDef, nullable: op.nullable };
        stmts.push(`${prefix} MODIFY COLUMN ${newColumnSnippet(merged, ctx)};`);
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
      case 'set_column_comment': {
        const colDef = ctx.table.columns.find(c => c.name === op.columnName);
        if (!colDef) {
          ctx.warnings.push(warn('internal.columnNotFound', { name: op.columnName }));
          break;
        }
        const merged: ColumnDef = { ...colDef, comment: op.comment };
        stmts.push(`${prefix} MODIFY COLUMN ${newColumnSnippet(merged, ctx)};`);
        break;
      }
      case 'set_table_comment':
        stmts.push(`${prefix} COMMENT=${quoteSqlString(op.comment)};`);
        break;
      case 'add_foreign_key':
        stmts.push(`${prefix} ADD ${fkSnippet(op.foreignKey, ctx)};`);
        break;
      case 'drop_foreign_key':
        stmts.push(`${prefix} DROP FOREIGN KEY ${quoteIdentifier(op.name, ctx.driver)};`);
        break;
      case 'add_check':
        stmts.push(`${prefix} ADD ${checkSnippet(op.check, ctx)};`);
        break;
      case 'drop_check':
        stmts.push(
          ctx.driver === Driver.Mariadb
            ? `${prefix} DROP CONSTRAINT ${quoteIdentifier(op.name, ctx.driver)};`
            : `${prefix} DROP CHECK ${quoteIdentifier(op.name, ctx.driver)};`
        );
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
