// SPDX-License-Identifier: Apache-2.0

import { Driver } from './drivers';

/**
 * Drivers whose connection strings can be parsed by the backend URL parser.
 *
 * Managed and compatible drivers intentionally reuse their wire protocol:
 * Wire-compatible drivers intentionally reuse their protocol's URL scheme.
 */
export const CONNECTION_URL_PLACEHOLDERS = {
  [Driver.Postgres]: 'postgresql://user:password@localhost:5432/mydb',
  [Driver.Mysql]: 'mysql://user:password@localhost:3306/mydb',
  [Driver.Mariadb]: 'mysql://user:password@localhost:3306/mydb',
  [Driver.PlanetScale]: 'mysql://user:password@aws.connect.psdb.cloud:3306/mydb?ssl-mode=required',
  [Driver.TiDb]: 'mysql://root:password@localhost:4000/test',
  [Driver.StarRocks]: 'mysql://root:password@localhost:9030/test',
  [Driver.Doris]: 'mysql://root:password@localhost:9030/test',
  [Driver.SingleStore]: 'mysql://admin:password@host:3306/mydb?ssl-mode=required',
  [Driver.Mongodb]: 'mongodb://user:password@localhost:27017/mydb',
  [Driver.DocumentDb]:
    'mongodb://user:password@docdb-cluster.cluster-id.region.docdb.amazonaws.com:27017/mydb?tls=true',
  [Driver.Redis]: 'redis://default:password@localhost:6379/0',
  [Driver.Valkey]: 'valkey://default:password@localhost:6379/0',
  [Driver.Dragonfly]: 'redis://default:password@localhost:6379/0',
  [Driver.KeyDb]: 'redis://default:password@localhost:6379/0',
  [Driver.Garnet]: 'redis://default:password@localhost:6379/0',
  [Driver.Motherduck]:
    'postgresql://postgres:motherduck_token@pg.<region>-aws.motherduck.com:5432/md:?sslmode=verify-full',
  [Driver.SqlServer]: 'sqlserver://user:password@localhost:1433/mydb?encrypt=true',
  [Driver.AzureSql]: 'sqlserver://user:password@server.database.windows.net:1433/mydb?encrypt=true',
  [Driver.Synapse]:
    'sqlserver://user:password@workspace.sql.azuresynapse.net:1433/mydb?encrypt=true',
  [Driver.Cockroachdb]: 'cockroachdb://user:password@host:26257/defaultdb?sslmode=verify-full',
  [Driver.YugabyteDb]: 'postgresql://yugabyte:password@host:5433/yugabyte?sslmode=verify-full',
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
