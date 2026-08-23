// SPDX-License-Identifier: Apache-2.0

import i18n from '@/i18n';

const SITE_BASE = 'https://www.qoredb.com';

export const COMMUNITY_LINKS = {
  github: 'https://github.com/QoreDB/QoreDB',
  issues: 'https://github.com/QoreDB/QoreDB/issues/new',
  contributors: 'https://github.com/QoreDB/QoreDB/graphs/contributors',
  discord: 'https://discord.gg/Yr6P3wuZDt',
} as const;

const DRIVER_DOC_SLUGS: Record<string, string> = {
  postgres: 'postgresql',
  supabase: 'postgresql',
  neon: 'postgresql',
  timescaledb: 'postgresql',
  mysql: 'mysql',
  mariadb: 'mysql',
  planetscale: 'mysql',
  mongodb: 'mongodb',
  documentdb: 'mongodb',
  redis: 'redis',
  valkey: 'redis',
  dragonfly: 'redis',
  sqlite: 'sqlite',
  duckdb: 'duckdb',
  motherduck: 'duckdb',
  sqlserver: 'sqlserver',
  cockroachdb: 'cockroachdb',
  clickhouse: 'clickhouse',
  elasticsearch: 'elasticsearch',
  opensearch: 'opensearch',
};

function getSiteLocale(): 'en' | 'fr' {
  return (i18n.language ?? '').toLowerCase().startsWith('fr') ? 'fr' : 'en';
}

export function getSiteUrl(path = ''): string {
  const suffix = path.replace(/^\/+|\/+$/g, '');
  return `${SITE_BASE}/${getSiteLocale()}${suffix ? `/${suffix}` : ''}`;
}

export function getDocsUrl(path = ''): string {
  const suffix = path.replace(/^\/+|\/+$/g, '');
  return getSiteUrl(`docs${suffix ? `/${suffix}` : ''}`);
}

export function getDriverDocsPath(driver: string): string {
  return `connections/${DRIVER_DOC_SLUGS[driver.toLowerCase()] ?? driver.toLowerCase()}`;
}
