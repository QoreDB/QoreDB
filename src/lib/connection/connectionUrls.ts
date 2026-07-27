// SPDX-License-Identifier: Apache-2.0

import { Driver } from './drivers';

/**
 * Drivers whose connection strings can be parsed by the backend URL parser.
 *
 * Managed and compatible drivers intentionally reuse their wire protocol:
 * MariaDB uses MySQL URLs, while Supabase, Neon, TimescaleDB and MotherDuck's
 * Postgres endpoint use PostgreSQL URLs.
 */
export const CONNECTION_URL_PLACEHOLDERS = {
  [Driver.Postgres]: 'postgresql://user:password@localhost:5432/mydb',
  [Driver.Mysql]: 'mysql://user:password@localhost:3306/mydb',
  [Driver.Mariadb]: 'mysql://user:password@localhost:3306/mydb',
  [Driver.Mongodb]: 'mongodb://user:password@localhost:27017/mydb',
  [Driver.Redis]: 'redis://default:password@localhost:6379/0',
  [Driver.Motherduck]:
    'postgresql://postgres:motherduck_token@pg.<region>-aws.motherduck.com:5432/md:?sslmode=verify-full',
  [Driver.SqlServer]: 'sqlserver://user:password@localhost:1433/mydb?encrypt=true',
  [Driver.Cockroachdb]: 'cockroachdb://user:password@host:26257/defaultdb?sslmode=verify-full',
  [Driver.Supabase]:
    'postgresql://postgres:password@db.project-ref.supabase.co:5432/postgres?sslmode=require',
  [Driver.Neon]:
    'postgresql://user:password@ep-example.region.aws.neon.tech/neondb?sslmode=require',
  [Driver.Timescaledb]: 'postgresql://user:password@host:5432/tsdb?sslmode=require',
} as const satisfies Partial<Record<Driver, string>>;

export type ConnectionUrlDriver = keyof typeof CONNECTION_URL_PLACEHOLDERS;

export function supportsConnectionUrl(driver: Driver): driver is ConnectionUrlDriver {
  return driver in CONNECTION_URL_PLACEHOLDERS;
}

export function getConnectionUrlPlaceholder(driver: Driver): string {
  return CONNECTION_URL_PLACEHOLDERS[driver as ConnectionUrlDriver];
}
