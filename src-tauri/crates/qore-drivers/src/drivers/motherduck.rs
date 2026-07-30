// SPDX-License-Identifier: Apache-2.0

//! MotherDuck Driver
//!
//! MotherDuck exposes a PostgreSQL wire-protocol endpoint
//! (`pg.<region>.motherduck.com:5432`, token as password) that runs DuckDB SQL
//! underneath. Transport therefore reuses the shared `pg_compat` helpers, but
//! schema introspection must go through the `duckdb_*` table functions: the
//! vanilla PostgreSQL introspection references catalogs DuckDB doesn't have
//! (`pg_matviews`, `pg_stat_user_tables`, ...), which is why a MotherDuck
//! connection routed through the Postgres driver connects yet shows an empty
//! schema explorer.

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use std::time::Instant;

use crate::drivers::pg_compat::{self, SessionMap};
use qore_core::error::{EngineError, EngineResult};
use qore_core::traits::{DataEngine, StreamSender};
use qore_core::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType,
    ConnectionConfig, ForeignKey, MaintenanceMessage, MaintenanceMessageLevel,
    MaintenanceOperationInfo, MaintenanceOperationType, MaintenanceRequest, MaintenanceResult,
    Namespace, PaginatedQueryResult, PaginationCapability, QueryId, QueryResult, Routine,
    RoutineDefinition, RoutineList, RoutineListOptions, RoutineOperationResult, RoutineType,
    RowData, Sequence, SequenceDefinition, SequenceList, SequenceListOptions,
    SequenceOperationResult, SessionId, SnapshotSupport, TableColumn, TableIndex,
    TableQueryOptions, TableSchema, Value,
};
use qore_sql::safety;

pub struct MotherDuckDriver {
    sessions: SessionMap,
}

const MACRO_ARGUMENTS_SQL: &str = r#"
    CASE WHEN len(parameters) = 0 THEN '' ELSE array_to_string(
        list_transform(
            range(1, len(parameters) + 1),
            i -> parameters[i] || CASE
                WHEN parameter_types[i] IS NULL
                  OR parameter_types[i] IN ('ANY', 'UNKNOWN') THEN ''
                ELSE ' ' || parameter_types[i]
            END
        ),
        ', '
    ) END
"#;

// `duckdb_schemas().internal` is true for every `main` schema, including the
// one a MotherDuck database keeps its tables in. Only the database-level flag
// separates user catalogs from `system` and `temp`.
const LIST_NAMESPACES_SQL: &str = r#"
    SELECT s.database_name, s.schema_name
    FROM duckdb_schemas() s
    JOIN duckdb_databases() d ON d.database_name = s.database_name
    WHERE d.internal = false
    ORDER BY s.database_name, s.schema_name
"#;

// The Postgres endpoint scopes its catalog views to the connected database, so
// `information_schema` cannot answer for any other catalog. The `duckdb_*`
// table functions carry `database_name` and stay correct across all of them.
const COUNT_COLLECTIONS_SQL: &str = r#"
    SELECT count(*) FROM (
        SELECT table_name AS name FROM duckdb_tables()
        WHERE database_name = $1 AND schema_name = $2
        UNION ALL
        SELECT view_name FROM duckdb_views()
        WHERE database_name = $1 AND schema_name = $2
    ) AS objects
    WHERE ($3::text IS NULL OR name ILIKE $3)
"#;

const LIST_COLLECTIONS_SQL: &str = r#"
    SELECT name, kind FROM (
        SELECT table_name AS name, 'BASE TABLE' AS kind FROM duckdb_tables()
        WHERE database_name = $1 AND schema_name = $2
        UNION ALL
        SELECT view_name, 'VIEW' FROM duckdb_views()
        WHERE database_name = $1 AND schema_name = $2
    ) AS objects
    WHERE ($3::text IS NULL OR name ILIKE $3)
    ORDER BY name
"#;

impl MotherDuckDriver {
    pub fn new() -> Self {
        Self {
            sessions: pg_compat::new_session_map(),
        }
    }

    /// MotherDuck's Postgres endpoint uses `md:` to select the account's
    /// default database. TLS is mandatory; default to full verification when
    /// the connection did not explicitly choose another secure SSL mode.
    fn conn_str(config: &ConnectionConfig) -> EngineResult<String> {
        let mut normalized = config.clone();
        if let Some(host) = motherduck_host_from_token(&normalized.password)
            && (normalized.host.is_empty()
                || (normalized.host.starts_with("pg.")
                    && normalized.host.ends_with(".motherduck.com")))
        {
            normalized.host = host;
        }
        let ssl_mode = normalized.ssl_mode.as_deref().map(str::to_ascii_lowercase);
        if matches!(ssl_mode.as_deref(), Some("disable" | "allow" | "prefer")) {
            return Err(EngineError::connection_failed(
                "MotherDuck's Postgres endpoint requires TLS (use sslmode=require or verify-full)",
            ));
        }
        normalized.ssl = true;
        if normalized.ssl_mode.is_none() {
            normalized.ssl_mode = Some("verify-full".into());
        }
        Ok(pg_compat::build_pg_connection_string(&normalized, "md:"))
    }

    fn ensure_safe_query(query: &str) -> EngineResult<()> {
        if let Some(danger) = safety::classify_duckdb_script_dangerous(query) {
            return Err(EngineError::not_supported(danger.reason()));
        }
        Ok(())
    }
}

fn motherduck_host_from_token(token: &str) -> Option<String> {
    let payload = token.trim().split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    let md_region = claims.get("mdRegion")?.as_str()?;
    let (provider, region) = md_region.split_once('-')?;

    if provider != "aws"
        || region.is_empty()
        || !region
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return None;
    }

    Some(format!(
        "pg.{}-aws.motherduck.com",
        region.to_ascii_lowercase()
    ))
}

fn split_catalog_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|item| item.trim().trim_matches('"').to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn extract_index_columns(sql: Option<&str>) -> Vec<String> {
    let Some(sql) = sql else {
        return Vec::new();
    };
    match (sql.rfind('('), sql.rfind(')')) {
        (Some(start), Some(end)) if start < end => split_catalog_list(&sql[start + 1..end]),
        _ => Vec::new(),
    }
}

fn qualified_object_name(namespace: &Namespace, name: &str) -> String {
    let schema = namespace.schema.as_deref().unwrap_or("main");
    format!(
        "{}.{}.{}",
        pg_compat::quote_ident(&namespace.database),
        pg_compat::quote_ident(schema),
        pg_compat::quote_ident(name)
    )
}

impl Default for MotherDuckDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataEngine for MotherDuckDriver {
    fn driver_id(&self) -> &'static str {
        "motherduck"
    }

    fn driver_name(&self) -> &'static str {
        "MotherDuck"
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        pg_compat::test_connection(&Self::conn_str(config)?).await
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        let mut pool_config = config.clone();
        // Do not keep idle cloud connections alive unless the user explicitly
        // requested a minimum. This lets MotherDuck scale to zero naturally.
        pool_config.pool_min_connections = Some(config.pool_min_connections.unwrap_or(0));
        pg_compat::connect(&self.sessions, &pool_config, &Self::conn_str(config)?).await
    }

    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        pg_compat::disconnect(&self.sessions, session).await
    }

    async fn ping(&self, session: SessionId) -> EngineResult<()> {
        pg_compat::ping(&self.sessions, session).await
    }

    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let pool = &pg.pool;

        let rows: Vec<(String, String)> = sqlx::query_as(LIST_NAMESPACES_SQL)
            .fetch_all(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|(database, schema)| Namespace::with_schema(database, schema))
            .collect())
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let pool = &pg.pool;

        let schema = namespace.schema.as_deref().unwrap_or("main");
        let search_pattern = options.search.as_ref().map(|s| format!("%{}%", s));

        let count_row: (i64,) = sqlx::query_as(COUNT_COLLECTIONS_SQL)
            .bind(&namespace.database)
            .bind(schema)
            .bind(&search_pattern)
            .fetch_one(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let mut query_str = LIST_COLLECTIONS_SQL.to_string();

        if let Some(limit) = options.page_size {
            query_str.push_str(&format!(" LIMIT {}", limit));
            if let Some(page) = options.page {
                let offset = (page.max(1) - 1) * limit;
                query_str.push_str(&format!(" OFFSET {}", offset));
            }
        }

        let rows: Vec<(String, String)> = sqlx::query_as(&query_str)
            .bind(&namespace.database)
            .bind(schema)
            .bind(&search_pattern)
            .fetch_all(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let collections = rows
            .into_iter()
            .map(|(name, table_type)| {
                let collection_type = if table_type.to_ascii_uppercase().contains("VIEW") {
                    CollectionType::View
                } else {
                    CollectionType::Table
                };
                Collection {
                    namespace: namespace.clone(),
                    name,
                    collection_type,
                }
            })
            .collect();

        Ok(CollectionList {
            collections,
            total_count: count_row.0 as u32,
        })
    }

    fn supports_routines(&self) -> bool {
        true
    }

    async fn list_routines(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: RoutineListOptions,
    ) -> EngineResult<RoutineList> {
        if matches!(options.routine_type, Some(RoutineType::Procedure)) {
            return Ok(RoutineList {
                routines: Vec::new(),
                total_count: 0,
            });
        }

        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let schema = namespace.schema.as_deref().unwrap_or("main");
        let search = options.search.as_ref().map(|value| format!("%{value}%"));
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*)
            FROM duckdb_functions()
            WHERE database_name = $1
              AND schema_name = $2
              AND internal = false
              AND macro_definition IS NOT NULL
              AND ($3::text IS NULL OR function_name ILIKE $3)
            "#,
        )
        .bind(&namespace.database)
        .bind(schema)
        .bind(&search)
        .fetch_one(&pg.pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let mut sql = format!(
            r#"
            SELECT function_name, function_type,
                   {MACRO_ARGUMENTS_SQL}, return_type
            FROM duckdb_functions()
            WHERE database_name = $1
              AND schema_name = $2
              AND internal = false
              AND macro_definition IS NOT NULL
              AND ($3::text IS NULL OR function_name ILIKE $3)
            ORDER BY function_name, function_oid
        "#
        );
        if let Some(page_size) = options.page_size {
            let page = options.page.unwrap_or(1).max(1);
            sql.push_str(&format!(
                " LIMIT {page_size} OFFSET {}",
                (page - 1) * page_size
            ));
        }

        let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(&sql)
            .bind(&namespace.database)
            .bind(schema)
            .bind(&search)
            .fetch_all(&pg.pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
        let namespace = namespace.clone();
        let routines = rows
            .into_iter()
            .map(|(name, function_type, arguments, return_type)| Routine {
                namespace: namespace.clone(),
                name,
                routine_type: RoutineType::Function,
                arguments,
                return_type: if function_type.contains("table") {
                    Some("TABLE".into())
                } else {
                    return_type
                },
                language: Some("SQL macro".into()),
            })
            .collect();

        Ok(RoutineList {
            routines,
            total_count: count.max(0) as u32,
        })
    }

    async fn get_routine_definition(
        &self,
        session: SessionId,
        namespace: &Namespace,
        routine_name: &str,
        routine_type: RoutineType,
        _arguments: Option<&str>,
    ) -> EngineResult<RoutineDefinition> {
        if routine_type == RoutineType::Procedure {
            return Err(EngineError::not_supported(
                "MotherDuck does not support stored procedures",
            ));
        }
        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let schema = namespace.schema.as_deref().unwrap_or("main");
        let definition_sql = format!(
            r#"
            SELECT function_type, {MACRO_ARGUMENTS_SQL},
                   return_type, macro_definition
            FROM duckdb_functions()
            WHERE database_name = $1
              AND schema_name = $2 AND function_name = $3
              AND internal = false AND macro_definition IS NOT NULL
            ORDER BY function_oid
            "#
        );
        let rows: Vec<(String, String, Option<String>, String)> = sqlx::query_as(&definition_sql)
            .bind(&namespace.database)
            .bind(schema)
            .bind(routine_name)
            .fetch_all(&pg.pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
        let candidate = rows
            .into_iter()
            .find(|(_, arguments, _, _)| _arguments.is_none_or(|requested| requested == arguments));
        let (function_type, arguments, return_type, body) = candidate.ok_or_else(|| {
            EngineError::execution_error(format!(
                "MotherDuck macro {}.{} was not found",
                schema, routine_name
            ))
        })?;
        let table_keyword = if function_type.contains("table") {
            " TABLE"
        } else {
            ""
        };
        let definition = format!(
            "CREATE MACRO {}({}) AS{} {};",
            qualified_object_name(namespace, routine_name),
            arguments,
            table_keyword,
            body
        );

        Ok(RoutineDefinition {
            name: routine_name.to_string(),
            namespace: namespace.clone(),
            routine_type: RoutineType::Function,
            definition,
            language: Some("SQL macro".into()),
            arguments,
            return_type: if function_type.contains("table") {
                Some("TABLE".into())
            } else {
                return_type
            },
        })
    }

    async fn drop_routine(
        &self,
        session: SessionId,
        namespace: &Namespace,
        routine_name: &str,
        routine_type: RoutineType,
        _arguments: Option<&str>,
    ) -> EngineResult<RoutineOperationResult> {
        if routine_type == RoutineType::Procedure {
            return Err(EngineError::not_supported(
                "MotherDuck does not support stored procedures",
            ));
        }
        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let sql = format!(
            "DROP MACRO {}",
            qualified_object_name(namespace, routine_name)
        );
        let start = Instant::now();
        sqlx::query(&sql)
            .execute(&pg.pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
        Ok(RoutineOperationResult {
            success: true,
            executed_command: sql,
            message: Some("MotherDuck macro dropped successfully".into()),
            execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
        })
    }

    fn supports_sequences(&self) -> bool {
        true
    }

    async fn list_sequences(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: SequenceListOptions,
    ) -> EngineResult<SequenceList> {
        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let schema = namespace.schema.as_deref().unwrap_or("main");
        let search = options.search.as_ref().map(|value| format!("%{value}%"));
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM duckdb_sequences()
            WHERE database_name = $1 AND schema_name = $2
              AND ($3::text IS NULL OR sequence_name ILIKE $3)
            "#,
        )
        .bind(&namespace.database)
        .bind(schema)
        .bind(&search)
        .fetch_one(&pg.pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let mut sql = r#"
            SELECT sequence_name, start_value, min_value, max_value, increment_by, cycle
            FROM duckdb_sequences()
            WHERE database_name = $1 AND schema_name = $2
              AND ($3::text IS NULL OR sequence_name ILIKE $3)
            ORDER BY sequence_name
        "#
        .to_string();
        if let Some(page_size) = options.page_size {
            let page = options.page.unwrap_or(1).max(1);
            sql.push_str(&format!(
                " LIMIT {page_size} OFFSET {}",
                (page - 1) * page_size
            ));
        }
        let rows: Vec<(String, i64, i64, i64, i64, bool)> = sqlx::query_as(&sql)
            .bind(&namespace.database)
            .bind(schema)
            .bind(&search)
            .fetch_all(&pg.pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
        let namespace = namespace.clone();
        let sequences = rows
            .into_iter()
            .map(
                |(name, start_value, min_value, max_value, increment, cycle)| Sequence {
                    namespace: namespace.clone(),
                    name,
                    data_type: "BIGINT".into(),
                    start_value,
                    min_value,
                    max_value,
                    increment,
                    cycle,
                    cache_size: 0,
                },
            )
            .collect();
        Ok(SequenceList {
            sequences,
            total_count: count.max(0) as u32,
        })
    }

    async fn get_sequence_definition(
        &self,
        session: SessionId,
        namespace: &Namespace,
        sequence_name: &str,
    ) -> EngineResult<SequenceDefinition> {
        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let schema = namespace.schema.as_deref().unwrap_or("main");
        let definition: Option<String> = sqlx::query_scalar(
            r#"
            SELECT sql FROM duckdb_sequences()
            WHERE database_name = $1
              AND schema_name = $2 AND sequence_name = $3
            "#,
        )
        .bind(&namespace.database)
        .bind(schema)
        .bind(sequence_name)
        .fetch_optional(&pg.pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;
        Ok(SequenceDefinition {
            name: sequence_name.to_string(),
            namespace: namespace.clone(),
            definition: definition.ok_or_else(|| {
                EngineError::execution_error(format!(
                    "MotherDuck sequence {}.{} was not found",
                    schema, sequence_name
                ))
            })?,
        })
    }

    async fn drop_sequence(
        &self,
        session: SessionId,
        namespace: &Namespace,
        sequence_name: &str,
    ) -> EngineResult<SequenceOperationResult> {
        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let sql = format!(
            "DROP SEQUENCE {}",
            qualified_object_name(namespace, sequence_name)
        );
        let start = Instant::now();
        sqlx::query(&sql)
            .execute(&pg.pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
        Ok(SequenceOperationResult {
            success: true,
            executed_command: sql,
            message: Some("MotherDuck sequence dropped successfully".into()),
            execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
        })
    }

    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let pool = &pg.pool;
        let schema = namespace.schema.as_deref().unwrap_or("main");

        let column_rows: Vec<(String, String, bool, Option<String>)> = sqlx::query_as(
            r#"
            SELECT column_name, data_type, is_nullable, column_default
            FROM duckdb_columns()
            WHERE database_name = $1
              AND schema_name = $2 AND table_name = $3
            ORDER BY column_index
            "#,
        )
        .bind(&namespace.database)
        .bind(schema)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        if column_rows.is_empty() {
            return Err(EngineError::execution_error(format!(
                "MotherDuck table or view {}.{} was not found",
                schema, table
            )));
        }

        // Constraints and indexes come from DuckDB table functions that a given
        // MotherDuck endpoint may not expose — treat them as best-effort so the
        // core column list still renders if they fail.
        let pk_columns: Vec<String> = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT unnest(constraint_column_names)
            FROM duckdb_constraints()
            WHERE database_name = $1
              AND schema_name = $2 AND table_name = $3
              AND constraint_type = 'PRIMARY KEY'
            "#,
        )
        .bind(&namespace.database)
        .bind(schema)
        .bind(table)
        .fetch_all(pool)
        .await
        .map(|rows| rows.into_iter().map(|(c,)| c).collect())
        .unwrap_or_default();

        let fk_rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT unnest(constraint_column_names), unnest(referenced_column_names),
                   referenced_table, constraint_name
            FROM duckdb_constraints()
            WHERE database_name = $1
              AND schema_name = $2 AND table_name = $3
              AND constraint_type = 'FOREIGN KEY'
            "#,
        )
        .bind(&namespace.database)
        .bind(schema)
        .bind(table)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let foreign_keys = fk_rows
            .into_iter()
            .map(
                |(column, referenced_column, referenced_table, constraint_name)| ForeignKey {
                    column,
                    referenced_table,
                    referenced_column,
                    referenced_schema: Some(schema.to_string()),
                    referenced_database: Some(namespace.database.clone()),
                    constraint_name,
                    is_virtual: false,
                },
            )
            .collect();

        let constraint_idx_rows: Vec<(String, String, String)> = sqlx::query_as(
            r#"
            SELECT constraint_name, constraint_type,
                   array_to_string(constraint_column_names, ',')
            FROM duckdb_constraints()
            WHERE database_name = $1
              AND schema_name = $2 AND table_name = $3
              AND constraint_type IN ('PRIMARY KEY', 'UNIQUE')
            "#,
        )
        .bind(&namespace.database)
        .bind(schema)
        .bind(table)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let mut indexes: Vec<TableIndex> = constraint_idx_rows
            .into_iter()
            .map(|(name, constraint_type, columns)| TableIndex {
                name,
                columns: split_catalog_list(&columns),
                is_unique: true,
                is_primary: constraint_type == "PRIMARY KEY",
                index_type: Some("ART".into()),
            })
            .collect();

        let idx_rows: Vec<(String, bool, Option<String>)> = sqlx::query_as(
            r#"
            SELECT index_name, is_unique, sql
            FROM duckdb_indexes()
            WHERE database_name = $1
              AND schema_name = $2 AND table_name = $3
            "#,
        )
        .bind(&namespace.database)
        .bind(schema)
        .bind(table)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        indexes.extend(
            idx_rows
                .into_iter()
                .map(|(name, is_unique, sql)| TableIndex {
                    name,
                    columns: extract_index_columns(sql.as_deref()),
                    is_unique,
                    is_primary: false,
                    index_type: Some("ART".into()),
                }),
        );

        let row_count_estimate: Option<u64> = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT estimated_size
            FROM duckdb_tables()
            WHERE database_name = $1
              AND schema_name = $2 AND table_name = $3
            "#,
        )
        .bind(&namespace.database)
        .bind(schema)
        .bind(table)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|c| if c < 0 { None } else { Some(c as u64) });

        let columns = column_rows
            .into_iter()
            .map(
                |(name, data_type, is_nullable, default_value)| TableColumn {
                    is_primary_key: pk_columns.contains(&name),
                    name,
                    data_type,
                    nullable: is_nullable,
                    is_auto_increment: default_value
                        .as_deref()
                        .is_some_and(|value| value.to_ascii_lowercase().contains("nextval(")),
                    default_value,
                },
            )
            .collect();

        Ok(TableSchema {
            columns,
            primary_key: if pk_columns.is_empty() {
                None
            } else {
                Some(pk_columns)
            },
            foreign_keys,
            row_count_estimate,
            indexes,
        })
    }

    async fn execute(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        Self::ensure_safe_query(query)?;
        pg_compat::execute_in_namespace(
            &self.sessions,
            self.driver_id(),
            session,
            None,
            query,
            query_id,
        )
        .await
    }

    async fn execute_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        Self::ensure_safe_query(query)?;
        pg_compat::execute_in_namespace(
            &self.sessions,
            self.driver_id(),
            session,
            namespace,
            query,
            query_id,
        )
        .await
    }

    async fn execute_stream(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
        sender: StreamSender,
    ) -> EngineResult<()> {
        Self::ensure_safe_query(query)?;
        pg_compat::execute_stream_in_namespace(
            &self.sessions,
            self.driver_id(),
            session,
            None,
            query,
            query_id,
            sender,
        )
        .await
    }

    async fn execute_stream_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        query_id: QueryId,
        sender: StreamSender,
    ) -> EngineResult<()> {
        Self::ensure_safe_query(query)?;
        pg_compat::execute_stream_in_namespace(
            &self.sessions,
            self.driver_id(),
            session,
            namespace,
            query,
            query_id,
            sender,
        )
        .await
    }

    async fn preview_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let query = format!(
            "SELECT * FROM {} LIMIT {}",
            qualified_object_name(namespace, table),
            limit
        );
        self.execute(session, &query, QueryId::new()).await
    }

    async fn query_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        options: TableQueryOptions,
    ) -> EngineResult<PaginatedQueryResult> {
        pg_compat::query_table_duckdb(&self.sessions, session, namespace, table, options).await
    }

    async fn peek_foreign_key(
        &self,
        session: SessionId,
        namespace: &Namespace,
        foreign_key: &ForeignKey,
        value: &Value,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        pg_compat::peek_foreign_key_duckdb(
            &self.sessions,
            session,
            namespace,
            foreign_key,
            value,
            limit,
        )
        .await
    }

    async fn cancel(&self, session: SessionId, query_id: Option<QueryId>) -> EngineResult<()> {
        pg_compat::cancel(&self.sessions, session, query_id).await
    }

    fn pagination_capability(&self) -> PaginationCapability {
        PaginationCapability {
            keyset: true,
            requires_unique_key: true,
            supports_backward: false,
            snapshot: SnapshotSupport::Transaction,
            max_offset_window: None,
        }
    }

    fn cancel_support(&self) -> CancelSupport {
        // The endpoint is a PostgreSQL protocol translation layer. Cancellation
        // uses pg_cancel_backend when exposed, but MotherDuck does not document
        // it as a hard compatibility guarantee.
        CancelSupport::BestEffort
    }

    async fn begin_transaction(&self, session: SessionId) -> EngineResult<()> {
        pg_compat::begin_transaction(&self.sessions, session).await
    }

    async fn commit(&self, session: SessionId) -> EngineResult<()> {
        pg_compat::commit(&self.sessions, session).await
    }

    async fn rollback(&self, session: SessionId) -> EngineResult<()> {
        pg_compat::rollback(&self.sessions, session).await
    }

    fn supports_transactions(&self) -> bool {
        true
    }

    async fn insert_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        pg_compat::insert_row_duckdb(&self.sessions, session, namespace, table, data).await
    }

    async fn update_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        pg_compat::update_row_duckdb(&self.sessions, session, namespace, table, primary_key, data)
            .await
    }

    async fn delete_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
    ) -> EngineResult<QueryResult> {
        pg_compat::delete_row_duckdb(&self.sessions, session, namespace, table, primary_key).await
    }

    fn supports_mutations(&self) -> bool {
        true
    }

    fn supports_maintenance(&self) -> bool {
        true
    }

    async fn list_maintenance_operations(
        &self,
        _session: SessionId,
        _namespace: &Namespace,
        _table: &str,
    ) -> EngineResult<Vec<MaintenanceOperationInfo>> {
        Ok(vec![MaintenanceOperationInfo {
            operation: MaintenanceOperationType::Analyze,
            is_heavy: false,
            has_options: false,
        }])
    }

    async fn run_maintenance(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        request: &MaintenanceRequest,
    ) -> EngineResult<MaintenanceResult> {
        if request.operation != MaintenanceOperationType::Analyze {
            return Err(EngineError::not_supported(
                "Only ANALYZE is supported for MotherDuck",
            ));
        }
        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let sql = format!("ANALYZE {}", qualified_object_name(namespace, table));
        let start = Instant::now();
        sqlx::query(&sql)
            .execute(&pg.pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
        Ok(MaintenanceResult {
            executed_command: sql,
            messages: vec![MaintenanceMessage {
                level: MaintenanceMessageLevel::Info,
                text: "MotherDuck statistics updated successfully".into(),
            }],
            execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
            success: true,
        })
    }

    async fn create_database(
        &self,
        session: SessionId,
        name: &str,
        _options: Option<Value>,
    ) -> EngineResult<()> {
        pg_compat::create_schema(&self.sessions, session, name, "MotherDuck").await
    }

    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        pg_compat::drop_schema(&self.sessions, session, name, "MotherDuck").await
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_explain(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> ConnectionConfig {
        ConnectionConfig {
            options: Default::default(),
            driver: "motherduck".to_string(),
            host: "pg.us-east-1-aws.motherduck.com".to_string(),
            port: 5432,
            username: "postgres".to_string(),
            password: "md_token".to_string(),
            database: None,
            ssl: true,
            ssl_mode: Some("verify-full".to_string()),
            environment: "production".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
            proxy: None,
            mssql_auth: None,
            clickhouse_cluster: None,
            search_auth_mode: None,
            ssl_ca_cert: None,
        }
    }

    #[test]
    fn motherduck_driver_identity() {
        let d = MotherDuckDriver::new();
        assert_eq!(d.driver_id(), "motherduck");
        assert_eq!(d.driver_name(), "MotherDuck");
    }

    #[test]
    fn motherduck_connection_string() {
        let cfg = make_config();
        let conn = MotherDuckDriver::conn_str(&cfg).unwrap();
        assert!(conn.contains("pg.us-east-1-aws.motherduck.com"));
        assert!(conn.contains(":5432"));
        assert!(conn.contains("sslmode=verify-full"));
    }

    #[test]
    fn motherduck_default_db_when_missing() {
        let cfg = make_config();
        let conn = MotherDuckDriver::conn_str(&cfg).unwrap();
        assert!(conn.contains("/md:?"));
    }

    #[test]
    fn motherduck_uses_the_token_region_for_official_endpoints() {
        let payload = URL_SAFE_NO_PAD.encode(r#"{"mdRegion":"aws-eu-central-1"}"#);
        let mut cfg = make_config();
        cfg.password = format!("header.{payload}.signature");

        let conn = MotherDuckDriver::conn_str(&cfg).unwrap();
        assert!(conn.contains("pg.eu-central-1-aws.motherduck.com"));
        assert!(!conn.contains("pg.us-east-1-aws.motherduck.com"));
    }

    #[test]
    fn motherduck_tls_is_secure_by_default_and_cannot_be_disabled() {
        let mut cfg = make_config();
        cfg.ssl = false;
        cfg.ssl_mode = None;
        let conn = MotherDuckDriver::conn_str(&cfg).unwrap();
        assert!(conn.contains("sslmode=verify-full"));

        cfg.ssl_mode = Some("disable".into());
        assert!(MotherDuckDriver::conn_str(&cfg).is_err());
    }

    #[test]
    fn motherduck_applies_duckdb_security_to_every_statement() {
        assert!(MotherDuckDriver::ensure_safe_query("SELECT 1").is_ok());
        assert!(MotherDuckDriver::ensure_safe_query("SELECT 1; INSTALL httpfs").is_err());
        assert!(
            MotherDuckDriver::ensure_safe_query(
                "SELECT ';' AS harmless; COPY (SELECT 1) TO '/tmp/leak.csv'"
            )
            .is_err()
        );
    }

    #[test]
    fn motherduck_exposes_duckdb_schema_objects_and_maintenance() {
        let driver = MotherDuckDriver::new();
        assert!(driver.supports_routines());
        assert!(driver.supports_sequences());
        assert!(driver.supports_maintenance());
        assert_eq!(driver.cancel_support(), CancelSupport::BestEffort);
    }

    #[test]
    fn motherduck_catalog_helpers_extract_columns() {
        assert_eq!(split_catalog_list("id, customer_id"), ["id", "customer_id"]);
        assert_eq!(
            extract_index_columns(Some(
                "CREATE INDEX orders_idx ON main.orders (\"customer_id\", \"status\")"
            )),
            ["customer_id", "status"]
        );
    }

    #[cfg(feature = "driver-duckdb")]
    #[test]
    fn motherduck_discovers_schemas_across_catalogs() {
        let conn = duckdb::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            ATTACH ':memory:' AS analytics;
            CREATE SCHEMA analytics.sales;
            ATTACH ':memory:' AS operations;
            CREATE TABLE operations.main.items (sku TEXT);
            "#,
        )
        .unwrap();

        let mut statement = conn.prepare(LIST_NAMESPACES_SQL).unwrap();
        let namespaces = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(namespaces.contains(&("analytics".into(), "sales".into())));
        // DuckDB flags every `main` as internal, and that is where a MotherDuck
        // database keeps its tables: dropping it empties the explorer.
        assert!(namespaces.contains(&("operations".into(), "main".into())));
        assert!(
            !namespaces
                .iter()
                .any(|(database, _)| database == "system" || database == "temp")
        );
    }

    #[cfg(feature = "driver-duckdb")]
    #[test]
    fn motherduck_lists_tables_and_views_of_any_catalog() {
        let conn = duckdb::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            ATTACH ':memory:' AS reporting;
            CREATE TABLE reporting.main.orders (id INTEGER);
            CREATE VIEW reporting.main.recent_orders AS SELECT * FROM reporting.main.orders;
            ATTACH ':memory:' AS other;
            CREATE TABLE other.main.decoy (id INTEGER);
            "#,
        )
        .unwrap();

        let mut statement = conn.prepare(LIST_COLLECTIONS_SQL).unwrap();
        let collections = statement
            .query_map(duckdb::params!["reporting", "main", None::<String>], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(
            collections,
            [
                ("orders".to_string(), "BASE TABLE".to_string()),
                ("recent_orders".to_string(), "VIEW".to_string()),
            ]
        );

        let mut count_statement = conn.prepare(COUNT_COLLECTIONS_SQL).unwrap();
        let total: i64 = count_statement
            .query_row(duckdb::params!["reporting", "main", None::<String>], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(total, 2);

        let filtered: i64 = count_statement
            .query_row(duckdb::params!["reporting", "main", Some("%recent%")], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(filtered, 1);
    }

    #[test]
    fn motherduck_qualifies_objects_with_catalog_and_schema() {
        let namespace = Namespace::with_schema("analytics-prod", "Sales");
        assert_eq!(
            qualified_object_name(&namespace, "order\"lines"),
            "\"analytics-prod\".\"Sales\".\"order\"\"lines\""
        );
    }
}
