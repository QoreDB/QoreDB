// SPDX-License-Identifier: Apache-2.0

#[cfg(any(feature = "driver-cassandra", feature = "driver-scylladb"))]
pub mod cassandra;
#[cfg(feature = "driver-clickhouse")]
pub mod clickhouse;
#[cfg(feature = "driver-cockroachdb")]
pub mod cockroachdb;
#[cfg(any(feature = "driver-cassandra", feature = "driver-scylladb"))]
pub mod cql;
#[cfg(feature = "driver-bigquery")]
pub mod bigquery;
#[cfg(feature = "driver-snowflake")]
pub mod snowflake;
#[cfg(any(feature = "driver-snowflake", feature = "driver-bigquery"))]
pub mod warehouse_compat;
#[cfg(feature = "driver-documentdb")]
pub mod documentdb;
#[cfg(feature = "driver-duckdb")]
pub mod duckdb;
#[cfg(feature = "driver-elasticsearch")]
pub mod elasticsearch;
#[cfg(feature = "driver-mariadb")]
pub mod mariadb;
#[cfg(any(feature = "driver-documentdb", feature = "driver-mongodb"))]
pub mod mongodb;
#[cfg(feature = "driver-motherduck")]
pub mod motherduck;
#[cfg(any(
    feature = "driver-doris",
    feature = "driver-mariadb",
    feature = "driver-mysql",
    feature = "driver-planetscale",
    feature = "driver-singlestore",
    feature = "driver-starrocks",
    feature = "driver-tidb"
))]
pub mod mysql;
#[cfg(feature = "driver-neon")]
pub mod neon;
#[cfg(feature = "driver-opensearch")]
pub mod opensearch;
#[cfg(feature = "sqlx-postgres")]
pub mod pg_compat;
#[cfg(feature = "driver-planetscale")]
pub mod planetscale;
#[cfg(any(feature = "driver-postgres", feature = "driver-yugabytedb"))]
pub mod postgres;
#[cfg(feature = "sqlx-postgres")]
pub mod postgres_utils;
#[cfg(any(
    feature = "driver-dragonfly",
    feature = "driver-garnet",
    feature = "driver-keydb",
    feature = "driver-redis",
    feature = "driver-valkey"
))]
pub mod redis;
#[cfg(any(feature = "driver-elasticsearch", feature = "driver-opensearch"))]
pub mod search_compat;
#[cfg(feature = "driver-sqlite")]
pub mod sqlite;
#[cfg(any(
    feature = "driver-azuresql",
    feature = "driver-sqlserver",
    feature = "driver-synapse"
))]
pub mod sqlserver;
#[cfg(feature = "driver-supabase")]
pub mod supabase;
#[cfg(feature = "driver-timescaledb")]
pub mod timescaledb;
