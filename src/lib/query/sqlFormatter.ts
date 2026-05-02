// SPDX-License-Identifier: Apache-2.0

import { format, type SqlLanguage } from 'sql-formatter';
import { Driver } from '../connection/drivers';

const DIALECT_MAP: Record<Driver, SqlLanguage> = {
  [Driver.Postgres]: 'postgresql',
  [Driver.Mysql]: 'mysql',
  [Driver.Mongodb]: 'sql',
  [Driver.Redis]: 'sql',
  [Driver.Sqlite]: 'sqlite',
  [Driver.SqlServer]: 'tsql',
  [Driver.Duckdb]: 'sql',
  [Driver.Cockroachdb]: 'postgresql',
  [Driver.Mariadb]: 'mysql',
  [Driver.Supabase]: 'postgresql',
  [Driver.Neon]: 'postgresql',
  [Driver.Timescaledb]: 'postgresql',
};

export function formatSql(query: string, driver: Driver): string {
  const language: SqlLanguage = DIALECT_MAP[driver] || 'sql';
  try {
    return format(query, {
      language,
      keywordCase: 'upper',
      indentStyle: 'standard',
    });
  } catch {
    return query;
  }
}
