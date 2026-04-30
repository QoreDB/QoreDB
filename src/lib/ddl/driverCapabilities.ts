// SPDX-License-Identifier: Apache-2.0

import { Driver } from '../drivers';

export type IndexMethodPlacement = 'before-columns' | 'after-columns' | 'none';

export interface DdlCapabilities {
  supportsForeignKeys: boolean;
  supportsCheckConstraints: boolean;
  supportsUniqueConstraint: boolean;

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

const SQLSERVER_CAPS: DdlCapabilities = {
  supportsForeignKeys: true,
  supportsCheckConstraints: true,
  supportsUniqueConstraint: true,
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

const CAPABILITIES: Record<Driver, DdlCapabilities> = {
  [Driver.Postgres]: POSTGRES_CAPS,
  [Driver.Cockroachdb]: POSTGRES_CAPS,
  [Driver.Mysql]: MYSQL_CAPS,
  [Driver.Mariadb]: MYSQL_CAPS,
  [Driver.Sqlite]: SQLITE_CAPS,
  [Driver.Duckdb]: DUCKDB_CAPS,
  [Driver.SqlServer]: SQLSERVER_CAPS,
  [Driver.Mongodb]: NO_DDL,
  [Driver.Redis]: NO_DDL,
};

export function getDdlCapabilities(driver: Driver): DdlCapabilities {
  return CAPABILITIES[driver] ?? NO_DDL;
}

export function quoteSqlString(value: string): string {
  return `'${value.replace(/'/g, "''")}'`;
}
