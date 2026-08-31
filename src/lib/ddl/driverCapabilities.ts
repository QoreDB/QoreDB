// SPDX-License-Identifier: Apache-2.0

import { Driver } from '../connection/drivers';

export type IndexMethodPlacement = 'before-columns' | 'after-columns' | 'none';

export interface DdlCapabilities {
  supportsForeignKeys: boolean;
  supportsCheckConstraints: boolean;
  supportsUniqueConstraint: boolean;
  /** Whether the primary key can be added/dropped through ALTER TABLE. */
  supportsAlterPrimaryKey: boolean;

  inlineColumnComments: boolean;
  separateColumnComments: boolean;
  inlineTableComment: boolean;
  separateTableComment: boolean;

  supportsIndexes: boolean;
  supportsUniqueIndex: boolean;
  supportsIndexMethod: boolean;
  indexMethodPlacement: IndexMethodPlacement;
  supportsPartialIndex: boolean;
}

const NO_DDL: DdlCapabilities = {
  supportsForeignKeys: false,
  supportsCheckConstraints: false,
  supportsUniqueConstraint: false,
  supportsAlterPrimaryKey: false,
  inlineColumnComments: false,
  separateColumnComments: false,
  inlineTableComment: false,
  separateTableComment: false,
  supportsIndexes: false,
  supportsUniqueIndex: false,
  supportsIndexMethod: false,
  indexMethodPlacement: 'none',
  supportsPartialIndex: false,
};

const POSTGRES_CAPS: DdlCapabilities = {
  supportsForeignKeys: true,
  supportsCheckConstraints: true,
  supportsUniqueConstraint: true,
  supportsAlterPrimaryKey: true,
  inlineColumnComments: false,
  separateColumnComments: true,
  inlineTableComment: false,
  separateTableComment: true,
  supportsIndexes: true,
  supportsUniqueIndex: true,
  supportsIndexMethod: true,
  indexMethodPlacement: 'before-columns',
  supportsPartialIndex: true,
};

const MYSQL_CAPS: DdlCapabilities = {
  supportsForeignKeys: true,
  supportsCheckConstraints: true,
  supportsUniqueConstraint: true,
  supportsAlterPrimaryKey: true,
  inlineColumnComments: true,
  separateColumnComments: false,
  inlineTableComment: true,
  separateTableComment: false,
  supportsIndexes: true,
  supportsUniqueIndex: true,
  supportsIndexMethod: true,
  indexMethodPlacement: 'after-columns',
  supportsPartialIndex: false,
};

const SQLITE_CAPS: DdlCapabilities = {
  supportsForeignKeys: true,
  supportsCheckConstraints: true,
  supportsUniqueConstraint: true,
  // SQLite can only change a primary key by rebuilding the table.
  supportsAlterPrimaryKey: false,
  inlineColumnComments: false,
  separateColumnComments: false,
  inlineTableComment: false,
  separateTableComment: false,
  supportsIndexes: true,
  supportsUniqueIndex: true,
  supportsIndexMethod: false,
  indexMethodPlacement: 'none',
  supportsPartialIndex: true,
};

const DUCKDB_CAPS: DdlCapabilities = {
  supportsForeignKeys: true,
  supportsCheckConstraints: true,
  supportsUniqueConstraint: true,
  // DuckDB's ALTER TABLE cannot add or drop constraints.
  supportsAlterPrimaryKey: false,
  inlineColumnComments: false,
  separateColumnComments: true,
  inlineTableComment: false,
  separateTableComment: true,
  supportsIndexes: true,
  supportsUniqueIndex: true,
  supportsIndexMethod: false,
  indexMethodPlacement: 'none',
  supportsPartialIndex: false,
};

const CLICKHOUSE_CAPS: DdlCapabilities = {
  supportsForeignKeys: false, // ClickHouse has no FK enforcement.
  supportsCheckConstraints: true, // CONSTRAINT … CHECK is supported on MergeTree.
  supportsUniqueConstraint: false,
  supportsAlterPrimaryKey: false, // The sorting key is fixed at table creation.
  inlineColumnComments: true,
  separateColumnComments: false,
  inlineTableComment: true,
  separateTableComment: false,
  supportsIndexes: true, // Data-skipping indices.
  supportsUniqueIndex: false,
  supportsIndexMethod: true, // INDEX … TYPE bloom_filter|minmax|set
  indexMethodPlacement: 'after-columns',
  supportsPartialIndex: false,
};

const SQLSERVER_CAPS: DdlCapabilities = {
  supportsForeignKeys: true,
  supportsCheckConstraints: true,
  supportsUniqueConstraint: true,
  supportsAlterPrimaryKey: true,
  inlineColumnComments: false,
  separateColumnComments: false,
  inlineTableComment: false,
  separateTableComment: false,
  supportsIndexes: true,
  supportsUniqueIndex: true,
  supportsIndexMethod: false,
  indexMethodPlacement: 'none',
  supportsPartialIndex: true,
};

const SINGLESTORE_CAPS: DdlCapabilities = {
  ...MYSQL_CAPS,
  supportsForeignKeys: false,
  supportsCheckConstraints: false,
  supportsUniqueConstraint: false,
  supportsAlterPrimaryKey: false,
  supportsUniqueIndex: false,
};

const CAPABILITIES: Record<Driver, DdlCapabilities> = {
  [Driver.Postgres]: POSTGRES_CAPS,
  [Driver.Cockroachdb]: POSTGRES_CAPS,
  [Driver.Mysql]: MYSQL_CAPS,
  [Driver.Mariadb]: MYSQL_CAPS,
  [Driver.PlanetScale]: MYSQL_CAPS,
  [Driver.TiDb]: MYSQL_CAPS,
  [Driver.StarRocks]: NO_DDL,
  [Driver.Doris]: NO_DDL,
  [Driver.SingleStore]: SINGLESTORE_CAPS,
  [Driver.Sqlite]: SQLITE_CAPS,
  [Driver.Duckdb]: DUCKDB_CAPS,
  [Driver.Motherduck]: DUCKDB_CAPS,
  [Driver.SqlServer]: SQLSERVER_CAPS,
  [Driver.AzureSql]: SQLSERVER_CAPS,
  [Driver.Synapse]: NO_DDL,
  [Driver.Mongodb]: NO_DDL,
  [Driver.Redis]: NO_DDL,
  [Driver.Valkey]: NO_DDL,
  [Driver.Dragonfly]: NO_DDL,
  [Driver.KeyDb]: NO_DDL,
  [Driver.Garnet]: NO_DDL,
  [Driver.DocumentDb]: NO_DDL,
  [Driver.Supabase]: POSTGRES_CAPS,
  [Driver.Neon]: POSTGRES_CAPS,
  [Driver.Timescaledb]: POSTGRES_CAPS,
  [Driver.YugabyteDb]: POSTGRES_CAPS,
  [Driver.Clickhouse]: CLICKHOUSE_CAPS,
  [Driver.Elasticsearch]: NO_DDL,
  [Driver.OpenSearch]: NO_DDL,
};

export function getDdlCapabilities(driver: Driver): DdlCapabilities {
  return CAPABILITIES[driver] ?? NO_DDL;
}

export function quoteSqlString(value: string): string {
  return `'${value.replace(/'/g, "''")}'`;
}
