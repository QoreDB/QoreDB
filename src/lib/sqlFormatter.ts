import { format, SqlLanguage } from 'sql-formatter';
import { Driver } from './drivers';

const DIALECT_MAP: Record<Driver, SqlLanguage> = {
  [Driver.Postgres]: 'postgresql',
  [Driver.Mysql]: 'mysql',
  [Driver.Mongodb]: 'sql',
  [Driver.Sqlite]: 'sqlite',
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
