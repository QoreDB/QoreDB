// SPDX-License-Identifier: Apache-2.0

import type { SqlLanguage } from 'sql-formatter';
import { Driver } from '../connection/drivers';

const DIALECT_MAP: Record<Driver, SqlLanguage> = {
  [Driver.Postgres]: 'postgresql',
  [Driver.Mysql]: 'mysql',
  [Driver.Mongodb]: 'sql',
  [Driver.Redis]: 'sql',
  [Driver.Valkey]: 'sql',
  [Driver.Sqlite]: 'sqlite',
  [Driver.SqlServer]: 'tsql',
  [Driver.Duckdb]: 'sql',
  [Driver.Motherduck]: 'sql',
  [Driver.Cockroachdb]: 'postgresql',
  [Driver.Mariadb]: 'mysql',
  [Driver.Supabase]: 'postgresql',
  [Driver.Neon]: 'postgresql',
  [Driver.Timescaledb]: 'postgresql',
  [Driver.Clickhouse]: 'sql',
  [Driver.Elasticsearch]: 'sql',
  [Driver.OpenSearch]: 'sql',
};

export async function formatSql(query: string, driver: Driver): Promise<string> {
  const language: SqlLanguage = DIALECT_MAP[driver] || 'sql';
  try {
    const { format } = await import('sql-formatter');
    return format(query, {
      language,
      keywordCase: 'upper',
      indentStyle: 'standard',
    });
  } catch {
    return query;
  }
}
