// SPDX-License-Identifier: Apache-2.0

//! Drivers whose raw-SQL DDL the migrations runner can apply and whose schema the
//! diff engine understands. Excludes document/KV/search stores and ClickHouse
//! (non-standard transactions/DDL). Postgres-compatible drivers are included and
//! normalized to the Postgres dialect by the DDL builders.

export const SCHEMA_MIGRATION_DRIVERS = new Set([
  'postgres',
  'cockroachdb',
  'mysql',
  'mariadb',
  'sqlite',
  'duckdb',
  'motherduck',
  'sqlserver',
  'timescaledb',
  'supabase',
  'neon',
]);

/** DDL auto-commits here, so a failed migration can't be fully rolled back. */
export const NON_TX_DDL_DRIVERS = new Set(['mysql', 'mariadb']);
