// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { Driver } from '../connection/drivers';
import { buildAlterTableSQL, diffTableDefinitions } from './alterTable';
import type { ColumnDef, ForeignKeyDef, IndexDef, TableDefinition } from './types';

function col(name: string, over: Partial<ColumnDef> = {}): ColumnDef {
  return {
    name,
    type: 'integer',
    nullable: true,
    isPrimaryKey: false,
    isUnique: false,
    ...over,
  };
}

function table(over: Partial<TableDefinition> = {}): TableDefinition {
  return {
    namespace: { database: 'app', schema: 'public' },
    tableName: 'users',
    columns: [col('id'), col('email', { type: 'text' })],
    ...over,
  };
}

const idx = (over: Partial<IndexDef> = {}): IndexDef => ({
  name: 'idx_email',
  columns: ['email'],
  ...over,
});

const fk = (over: Partial<ForeignKeyDef> = {}): ForeignKeyDef => ({
  name: 'fk_owner',
  columns: ['owner_id'],
  refTable: 'accounts',
  refColumns: ['id'],
  ...over,
});

describe('primary key', () => {
  it('emits no op when the primary key is unchanged', () => {
    const before = table({ columns: [col('id', { isPrimaryKey: true })] });
    const after = table({ columns: [col('id', { isPrimaryKey: true })] });
    expect(diffTableDefinitions(before, after)).toEqual([]);
  });

  it('detects a primary key being added', () => {
    const before = table({ columns: [col('id')] });
    const after = table({ columns: [col('id', { isPrimaryKey: true })] });
    expect(diffTableDefinitions(before, after)).toEqual([
      { kind: 'add_primary_key', columns: ['id'] },
    ]);
  });

  it('detects a primary key being dropped, carrying the constraint name', () => {
    const before = table({
      columns: [col('id', { isPrimaryKey: true })],
      primaryKeyName: 'users_pkey',
    });
    const after = table({ columns: [col('id')] });
    expect(diffTableDefinitions(before, after)).toEqual([
      { kind: 'drop_primary_key', name: 'users_pkey' },
    ]);
  });

  it('drops before adding when the primary key moves to another column', () => {
    const before = table({
      columns: [col('id', { isPrimaryKey: true }), col('email')],
      primaryKeyName: 'users_pkey',
    });
    const after = table({ columns: [col('id'), col('email', { isPrimaryKey: true })] });
    expect(diffTableDefinitions(before, after)).toEqual([
      { kind: 'drop_primary_key', name: 'users_pkey' },
      { kind: 'add_primary_key', columns: ['email'] },
    ]);
  });

  it('detects a composite primary key change', () => {
    const before = table({ columns: [col('a', { isPrimaryKey: true }), col('b')] });
    const after = table({
      columns: [col('a', { isPrimaryKey: true }), col('b', { isPrimaryKey: true })],
    });
    const ops = diffTableDefinitions(before, after);
    expect(ops).toContainEqual({ kind: 'add_primary_key', columns: ['a', 'b'] });
  });
});

describe('primary key SQL', () => {
  const dropPk = table({
    columns: [col('id', { isPrimaryKey: true })],
    primaryKeyName: 'users_pkey',
  });

  it('drops by constraint name on postgres', () => {
    const res = buildAlterTableSQL(
      dropPk,
      [{ kind: 'drop_primary_key', name: 'users_pkey' }],
      Driver.Postgres
    );
    expect(res.statements).toEqual(['ALTER TABLE "public"."users" DROP CONSTRAINT "users_pkey";']);
  });

  it('uses dedicated syntax on mysql, needing no name', () => {
    const res = buildAlterTableSQL(
      dropPk,
      [{ kind: 'drop_primary_key', name: undefined }],
      Driver.Mysql
    );
    expect(res.statements).toEqual(['ALTER TABLE `app`.`users` DROP PRIMARY KEY;']);
  });

  it('adds a primary key on postgres', () => {
    const res = buildAlterTableSQL(
      dropPk,
      [{ kind: 'add_primary_key', columns: ['id'] }],
      Driver.Postgres
    );
    expect(res.statements).toEqual(['ALTER TABLE "public"."users" ADD PRIMARY KEY ("id");']);
  });

  it('names the constraint on sql server', () => {
    const res = buildAlterTableSQL(
      dropPk,
      [{ kind: 'add_primary_key', columns: ['id'] }],
      Driver.SqlServer
    );
    expect(res.statements[0]).toContain('ADD CONSTRAINT [PK_users] PRIMARY KEY ([id])');
  });

  it('warns instead of emitting SQL on sqlite', () => {
    const res = buildAlterTableSQL(
      dropPk,
      [{ kind: 'add_primary_key', columns: ['id'] }],
      Driver.Sqlite
    );
    expect(res.statements).toEqual([]);
    expect(res.warnings.map(w => w.code)).toContain('pk.alterUnsupported');
  });

  it('warns when dropping a primary key whose name is unknown', () => {
    const res = buildAlterTableSQL(
      table({ columns: [col('id', { isPrimaryKey: true })] }),
      [{ kind: 'drop_primary_key', name: undefined }],
      Driver.Postgres
    );
    expect(res.statements).toEqual([]);
    expect(res.warnings.map(w => w.code)).toContain('pk.dropRequiresName');
  });
});

describe('index SQL', () => {
  const t = table({ indexes: [idx()] });

  it('scopes the drop to the table on sql server', () => {
    // T-SQL index names are scoped to their table, so a bare DROP INDEX fails.
    const res = buildAlterTableSQL(
      t,
      [{ kind: 'drop_index', name: 'idx_email' }],
      Driver.SqlServer
    );
    expect(res.statements).toEqual(['DROP INDEX [idx_email] ON [public].[users];']);
  });

  it('scopes the drop to the table on mysql', () => {
    const res = buildAlterTableSQL(t, [{ kind: 'drop_index', name: 'idx_email' }], Driver.Mysql);
    expect(res.statements).toEqual(['DROP INDEX `idx_email` ON `app`.`users`;']);
  });

  it('drops by name alone on postgres', () => {
    const res = buildAlterTableSQL(t, [{ kind: 'drop_index', name: 'idx_email' }], Driver.Postgres);
    expect(res.statements).toEqual(['DROP INDEX "idx_email";']);
  });
});

describe('indexes', () => {
  it('emits no op for an identical index', () => {
    const before = table({ indexes: [idx()] });
    const after = table({ indexes: [idx()] });
    expect(diffTableDefinitions(before, after)).toEqual([]);
  });

  it('replaces a same-named index whose columns changed', () => {
    const before = table({ indexes: [idx({ columns: ['email'] })] });
    const after = table({ indexes: [idx({ columns: ['email', 'id'] })] });
    expect(diffTableDefinitions(before, after)).toEqual([
      { kind: 'drop_index', name: 'idx_email' },
      { kind: 'add_index', index: idx({ columns: ['email', 'id'] }) },
    ]);
  });

  it('replaces a same-named index whose uniqueness changed', () => {
    const before = table({ indexes: [idx({ unique: false })] });
    const after = table({ indexes: [idx({ unique: true })] });
    const ops = diffTableDefinitions(before, after);
    expect(ops.map(o => o.kind)).toEqual(['drop_index', 'add_index']);
  });

  it('replaces a same-named index whose method changed', () => {
    const before = table({ indexes: [idx({ method: 'btree' })] });
    const after = table({ indexes: [idx({ method: 'hash' })] });
    expect(diffTableDefinitions(before, after).map(o => o.kind)).toEqual([
      'drop_index',
      'add_index',
    ]);
  });

  it('orders the drop before the add so the name is free', () => {
    const before = table({ indexes: [idx({ unique: false })] });
    const after = table({ indexes: [idx({ unique: true })] });
    const ops = diffTableDefinitions(before, after);
    expect(ops.findIndex(o => o.kind === 'drop_index')).toBeLessThan(
      ops.findIndex(o => o.kind === 'add_index')
    );
  });
});

describe('foreign keys', () => {
  it('emits no op for an identical foreign key', () => {
    const before = table({ foreignKeys: [fk()] });
    const after = table({ foreignKeys: [fk()] });
    expect(diffTableDefinitions(before, after)).toEqual([]);
  });

  it('replaces a same-named foreign key pointing somewhere else', () => {
    const before = table({ foreignKeys: [fk({ refTable: 'accounts' })] });
    const after = table({ foreignKeys: [fk({ refTable: 'orgs' })] });
    expect(diffTableDefinitions(before, after)).toEqual([
      { kind: 'drop_foreign_key', name: 'fk_owner' },
      { kind: 'add_foreign_key', foreignKey: fk({ refTable: 'orgs' }) },
    ]);
  });

  it('replaces a same-named foreign key whose columns changed', () => {
    const before = table({ foreignKeys: [fk({ columns: ['owner_id'] })] });
    const after = table({ foreignKeys: [fk({ columns: ['account_id'] })] });
    expect(diffTableDefinitions(before, after).map(o => o.kind)).toEqual([
      'drop_foreign_key',
      'add_foreign_key',
    ]);
  });
});

describe('operation order', () => {
  const kinds = (ops: ReturnType<typeof diffTableDefinitions>) => ops.map(o => o.kind);

  it('drops dependent objects before the column they cover', () => {
    // Dropping the column first is either refused for dependency reasons, or
    // takes the index with it and makes the later DROP INDEX fail.
    const before = table({
      columns: [col('id', { isPrimaryKey: true }), col('email')],
      primaryKeyName: 'users_pkey',
      indexes: [idx({ name: 'idx_email', columns: ['email'] })],
      foreignKeys: [fk({ columns: ['email'] })],
    });
    const after = table({
      columns: [col('id', { isPrimaryKey: true })],
      primaryKeyName: 'users_pkey',
    });
    const ops = kinds(diffTableDefinitions(before, after));

    const dropCol = ops.indexOf('drop_column');
    expect(ops.indexOf('drop_index')).toBeLessThan(dropCol);
    expect(ops.indexOf('drop_foreign_key')).toBeLessThan(dropCol);
  });

  it('creates a column before the index that covers it', () => {
    const before = table({ columns: [col('id')] });
    const after = table({
      columns: [col('id'), col('email', { type: 'text' })],
      indexes: [idx({ columns: ['email'] })],
    });
    const ops = kinds(diffTableDefinitions(before, after));
    expect(ops.indexOf('add_column')).toBeLessThan(ops.indexOf('add_index'));
  });

  it('renames a column before changing it', () => {
    const before = table({ columns: [col('mail', { type: 'text' })] });
    const after = table({ columns: [col('email', { type: 'varchar' })] });
    const ops = kinds(
      diffTableDefinitions(before, after, { columnRenames: [{ from: 'mail', to: 'email' }] })
    );
    expect(ops.indexOf('rename_column')).toBeLessThan(ops.indexOf('change_type'));
  });

  it('renames the table last, since every other statement uses the old name', () => {
    const before = table({ columns: [col('id')] });
    const after = table({ tableName: 'people', columns: [col('id'), col('name')] });
    const ops = kinds(
      diffTableDefinitions(before, after, { tableRename: { from: 'users', to: 'people' } })
    );
    expect(ops[ops.length - 1]).toBe('rename_table');
  });
});

describe('regressions already covered before the fix', () => {
  it('still detects an added column', () => {
    const before = table({ columns: [col('id')] });
    const after = table({ columns: [col('id'), col('name', { type: 'text' })] });
    expect(diffTableDefinitions(before, after)).toEqual([
      { kind: 'add_column', column: col('name', { type: 'text' }) },
    ]);
  });

  it('still detects a dropped column', () => {
    const before = table({ columns: [col('id'), col('name')] });
    const after = table({ columns: [col('id')] });
    expect(diffTableDefinitions(before, after)).toEqual([
      { kind: 'drop_column', columnName: 'name' },
    ]);
  });
});
