// SPDX-License-Identifier: Apache-2.0

import { Driver } from '../../drivers';
import { buildColumnSQL } from '../createTable';
import { getDdlCapabilities } from '../driverCapabilities';
import { buildQualifiedTableName, quoteIdentifier } from '../identifiers';
import type {
  CheckConstraintDef,
  ColumnDef,
  ForeignKeyDef,
  IndexDef,
  TableDefinition,
} from '../types';
import { type DdlWarning, warn } from '../warnings';

export interface BuilderContext {
  table: TableDefinition;
  driver: Driver;
  fullName: string;
  warnings: DdlWarning[];
}

export function newColumnSnippet(col: ColumnDef, ctx: BuilderContext): string {
  const caps = getDdlCapabilities(ctx.driver);
  return buildColumnSQL(col, ctx.driver, {
    suppressInlinePk: true,
    inlineComment: caps.inlineColumnComments,
  });
}

export function fkSnippet(fk: ForeignKeyDef, ctx: BuilderContext): string {
  const cols = fk.columns.map(c => quoteIdentifier(c, ctx.driver)).join(', ');
  const refCols = fk.refColumns.map(c => quoteIdentifier(c, ctx.driver)).join(', ');
  const refQualified = buildQualifiedTableName(
    { database: ctx.table.namespace.database, schema: fk.refSchema ?? ctx.table.namespace.schema },
    fk.refTable,
    ctx.driver
  );
  let s = '';
  if (fk.name) s += `CONSTRAINT ${quoteIdentifier(fk.name, ctx.driver)} `;
  s += `FOREIGN KEY (${cols}) REFERENCES ${refQualified} (${refCols})`;
  if (fk.onDelete) s += ` ON DELETE ${fk.onDelete}`;
  if (fk.onUpdate) s += ` ON UPDATE ${fk.onUpdate}`;
  return s;
}

export function checkSnippet(c: CheckConstraintDef, ctx: BuilderContext): string {
  let s = '';
  if (c.name) s += `CONSTRAINT ${quoteIdentifier(c.name, ctx.driver)} `;
  s += `CHECK (${c.expression})`;
  return s;
}

export function buildIndexStmt(idx: IndexDef, ctx: BuilderContext): string | null {
  const caps = getDdlCapabilities(ctx.driver);
  if (!caps.supportsIndexes) {
    ctx.warnings.push(warn('indexes.unsupported', { name: idx.name }));
    return null;
  }
  if (idx.unique && !caps.supportsUniqueIndex) {
    ctx.warnings.push(warn('indexes.uniqueDowngraded', { name: idx.name }));
  }
  const cols = idx.columns.map(c => quoteIdentifier(c, ctx.driver)).join(', ');
  const unique = idx.unique && caps.supportsUniqueIndex ? 'UNIQUE ' : '';
  const name = quoteIdentifier(idx.name, ctx.driver);
  let stmt = `CREATE ${unique}INDEX ${name} ON ${ctx.fullName}`;
  if (idx.method) {
    if (!caps.supportsIndexMethod) {
      ctx.warnings.push(warn('indexes.methodIgnored', { name: idx.name, method: idx.method }));
    } else if (caps.indexMethodPlacement === 'before-columns') {
      stmt += ` USING ${idx.method}`;
    }
  }
  stmt += ` (${cols})`;
  if (idx.method && caps.supportsIndexMethod && caps.indexMethodPlacement === 'after-columns') {
    stmt += ` USING ${idx.method}`;
  }
  if (idx.where) {
    if (caps.supportsPartialIndex) stmt += ` WHERE ${idx.where}`;
    else ctx.warnings.push(warn('indexes.partialUnsupported', { name: idx.name }));
  }
  return `${stmt};`;
}

export function dropIndexStmt(name: string, ctx: BuilderContext): string {
  const ident = quoteIdentifier(name, ctx.driver);
  if (ctx.driver === Driver.Mysql || ctx.driver === Driver.Mariadb) {
    return `DROP INDEX ${ident} ON ${ctx.fullName};`;
  }
  return `DROP INDEX ${ident};`;
}

export function alterPrefix(ctx: BuilderContext): string {
  return `ALTER TABLE ${ctx.fullName}`;
}
