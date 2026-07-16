// SPDX-License-Identifier: BUSL-1.1

import { describe, expect, it, vi } from 'vitest';
import { Driver } from '../connection/drivers';
import type { ColumnDef, TableDefinition } from '../ddl';

// `captureSnapshot` reaches for Tauri at module load; the pure generators don't.
vi.mock('@/lib/tauri', () => ({
  listNamespaces: vi.fn(),
  listCollections: vi.fn(),
  describeTable: vi.fn(),
}));

const { generateMigration, invertOps } = await import('./schemaDiff');
type SchemaSnapshot = Awaited<
  ReturnType<typeof import('./schemaDiff').captureSnapshot>
>['snapshot'];

function col(name: string, over: Partial<ColumnDef> = {}): ColumnDef {
  return { name, type: 'integer', nullable: true, isPrimaryKey: false, isUnique: false, ...over };
}

function def(over: Partial<TableDefinition> = {}): TableDefinition {
  return {
    namespace: { database: 'app', schema: 'public' },
    tableName: 'users',
    columns: [col('id')],
    ...over,
  };
}

function snap(tables: Record<string, TableDefinition>, driver = Driver.Postgres): SchemaSnapshot {
  return { capturedAt: '2026-01-01T00:00:00Z', driver, database: 'app', tables };
}

const KEY = 'app.public.users';

describe('generateMigration', () => {
  it('reports identical schemas as empty', () => {
    const s = snap({ [KEY]: def() });
    const res = generateMigration(s, s, Driver.Postgres);
    expect(res.isEmpty).toBe(true);
    expect(res.unexpressed).toEqual([]);
  });

  it('generates ADD COLUMN in up and its inverse in down', () => {
    const before = snap({ [KEY]: def({ columns: [col('id')] }) });
    const after = snap({ [KEY]: def({ columns: [col('id'), col('email', { type: 'text' })] }) });
    const res = generateMigration(before, after, Driver.Postgres);

    expect(res.isEmpty).toBe(false);
    expect(res.up).toContain('ADD COLUMN "email" text');
    expect(res.down).toContain('DROP COLUMN "email"');
    expect(res.hasIrreversible).toBe(false);
  });

  it('marks a dropped column as irreversible and comments the down out', () => {
    const before = snap({ [KEY]: def({ columns: [col('id'), col('email', { type: 'text' })] }) });
    const after = snap({ [KEY]: def({ columns: [col('id')] }) });
    const res = generateMigration(before, after, Driver.Postgres);

    expect(res.up).toContain('DROP COLUMN "email"');
    expect(res.hasIrreversible).toBe(true);
    // The re-add would restore the column but not its data, so it must not run.
    expect(res.down).toContain('-- ALTER TABLE');
    expect(res.down).toContain('IRREVERSIBLE');
  });

  it('reports SQLite type changes as unexpressed rather than as no changes', () => {
    const before = snap(
      { [KEY]: def({ columns: [col('id', { type: 'integer' })] }) },
      Driver.Sqlite
    );
    const after = snap({ [KEY]: def({ columns: [col('id', { type: 'text' })] }) }, Driver.Sqlite);
    const res = generateMigration(before, after, Driver.Sqlite);

    // The schemas differ, so this is NOT "no changes" even though no SQL came out.
    expect(res.isEmpty).toBe(false);
    expect(res.up).toBe('');
    expect(res.unexpressed.map(o => o.kind)).toContain('change_type');
    expect(res.warnings.map(w => w.code)).toContain('sqlite.alterColumnUnsupported');
  });

  it('refuses a SQLite table mixing expressible and inexpressible changes', () => {
    // ADD COLUMN works on SQLite; changing a column's type does not. Emitting
    // only the half that compiles would ship a silently incomplete migration.
    const before = snap(
      { [KEY]: def({ columns: [col('id', { type: 'integer' })] }) },
      Driver.Sqlite
    );
    const after = snap(
      { [KEY]: def({ columns: [col('id', { type: 'text' }), col('email', { type: 'text' })] }) },
      Driver.Sqlite
    );
    const res = generateMigration(before, after, Driver.Sqlite);

    expect(res.up).toContain('ADD COLUMN');
    expect(res.isEmpty).toBe(false);
    expect(res.unexpressed.map(o => o.kind)).toEqual(['change_type']);
  });

  it('reports no unexpressed ops when the dialect covers everything', () => {
    const before = snap({ [KEY]: def({ columns: [col('id', { type: 'integer' })] }) });
    const after = snap({ [KEY]: def({ columns: [col('id', { type: 'bigint' })] }) });
    expect(generateMigration(before, after, Driver.Postgres).unexpressed).toEqual([]);
  });

  it('normalizes motherduck to the duckdb dialect', () => {
    const before = snap({ [KEY]: def({ columns: [col('id')] }) }, Driver.Motherduck);
    const after = snap({ [KEY]: def({ columns: [col('id'), col('n')] }) }, Driver.Motherduck);
    const res = generateMigration(before, after, Driver.Motherduck);

    // Without normalization this hit the builder's default branch: empty SQL.
    expect(res.up).toContain('ADD COLUMN');
    expect(res.warnings.map(w => w.code)).not.toContain('alter.driverUnsupported');
  });

  it('normalizes supabase to the postgres dialect', () => {
    const before = snap({ [KEY]: def({ columns: [col('id')] }) }, Driver.Supabase);
    const after = snap({ [KEY]: def({ columns: [col('id'), col('n')] }) }, Driver.Supabase);
    expect(generateMigration(before, after, Driver.Supabase).up).toContain('ADD COLUMN');
  });

  it('creates an added table in up and drops it in down', () => {
    const before = snap({});
    const after = snap({ [KEY]: def() });
    const res = generateMigration(before, after, Driver.Postgres);
    expect(res.up).toContain('CREATE TABLE');
    expect(res.down).toContain('DROP TABLE');
    expect(res.hasIrreversible).toBe(false);
  });

  it('treats a dropped table as irreversible', () => {
    const before = snap({ [KEY]: def() });
    const after = snap({});
    const res = generateMigration(before, after, Driver.Postgres);
    expect(res.up).toContain('DROP TABLE');
    expect(res.hasIrreversible).toBe(true);
  });

  it('emits primary key SQL when the key moves', () => {
    const before = snap({
      [KEY]: def({
        columns: [col('id', { isPrimaryKey: true }), col('email')],
        primaryKeyName: 'users_pkey',
      }),
    });
    const after = snap({
      [KEY]: def({ columns: [col('id'), col('email', { isPrimaryKey: true })] }),
    });
    const res = generateMigration(before, after, Driver.Postgres);
    expect(res.up).toContain('DROP CONSTRAINT "users_pkey"');
    expect(res.up).toContain('ADD PRIMARY KEY ("email")');
  });
});

describe('invertOps', () => {
  it('treats re-adding a dropped column as lossy', () => {
    const before = def({ columns: [col('id'), col('email', { type: 'text' })] });
    const after = def({ columns: [col('id')] });
    const { reversible, irreversible } = invertOps(before, after);
    expect(irreversible.map(o => o.kind)).toEqual(['add_column']);
    expect(reversible).toEqual([]);
  });

  it('treats dropping an added column as reversible', () => {
    const before = def({ columns: [col('id')] });
    const after = def({ columns: [col('id'), col('email')] });
    const { reversible, irreversible } = invertOps(before, after);
    expect(reversible.map(o => o.kind)).toEqual(['drop_column']);
    expect(irreversible).toEqual([]);
  });

  it('treats a primary key change as reversible — a constraint, not data', () => {
    const before = def({ columns: [col('id', { isPrimaryKey: true })] });
    const after = def({ columns: [col('id')] });
    const { reversible, irreversible } = invertOps(before, after);
    expect(reversible.map(o => o.kind)).toEqual(['add_primary_key']);
    expect(irreversible).toEqual([]);
  });
});
