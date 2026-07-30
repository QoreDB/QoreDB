// SPDX-License-Identifier: Apache-2.0

use qoredb_lib::engine::{
    drivers::{
        clickhouse::ClickHouseDriver, duckdb::DuckDbDriver, elasticsearch::ElasticsearchDriver,
        mongodb::MongoDriver, mysql::MySqlDriver, postgres::PostgresDriver, redis::RedisDriver,
        sqlite::SqliteDriver,
    },
    error::{EngineError, EngineResult},
    traits::DataEngine,
    types::{
        CollectionListOptions, ConnectionConfig, CountMode, Namespace, OrderingGuarantee,
        PaginationStrategy, QueryId, RowData, SessionId, SortDirection, TableQueryOptions, Value,
    },
};
use serde_json::json;
use std::sync::Arc;
use tokio::time::{Duration, sleep, timeout};
use uuid::Uuid;

const DEFAULT_DB: &str = "testdb";

fn env_or_default(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_u16_or_default(key: &str, default: u16) -> u16 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(default)
}

fn env_bool_or_default(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn redis_test_required() -> bool {
    env_bool_or_default("QOREDB_TEST_REDIS_REQUIRED", false)
}

fn search_test_required() -> bool {
    env_bool_or_default("QOREDB_TEST_SEARCH_REQUIRED", false)
}

fn is_service_unavailable(err: &EngineError) -> bool {
    match err {
        EngineError::ConnectionFailed { message } | EngineError::ExecutionError { message } => {
            let lower = message.to_ascii_lowercase();
            lower.contains("connection refused")
                || lower.contains("no route to host")
                || lower.contains("timed out")
                || lower.contains("network is unreachable")
                || lower.contains("cannot assign requested address")
                // reqwest keeps the real cause in `source()`, so an HTTP driver
                // facing a dead port only ever reports this.
                || lower.contains("error sending request")
        }
        _ => false,
    }
}

fn postgres_config() -> ConnectionConfig {
    ConnectionConfig {
        options: Default::default(),
        driver: "postgres".to_string(),
        host: env_or_default("QOREDB_TEST_PG_HOST", "127.0.0.1"),
        port: env_u16_or_default("QOREDB_TEST_PG_PORT", 54321),
        username: env_or_default("QOREDB_TEST_PG_USER", "qoredb"),
        password: env_or_default("QOREDB_TEST_PG_PASSWORD", "qoredb_test"),
        database: Some(env_or_default("QOREDB_TEST_PG_DB", DEFAULT_DB)),
        ssl: false,
        ssl_mode: None,
        environment: "development".to_string(),
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

fn mysql_config() -> ConnectionConfig {
    ConnectionConfig {
        options: Default::default(),
        driver: "mysql".to_string(),
        host: env_or_default("QOREDB_TEST_MYSQL_HOST", "127.0.0.1"),
        port: env_u16_or_default("QOREDB_TEST_MYSQL_PORT", 3306),
        username: env_or_default("QOREDB_TEST_MYSQL_USER", "qoredb"),
        password: env_or_default("QOREDB_TEST_MYSQL_PASSWORD", "qoredb_test"),
        database: Some(env_or_default("QOREDB_TEST_MYSQL_DB", DEFAULT_DB)),
        ssl: false,
        ssl_mode: None,
        environment: "development".to_string(),
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

fn mongo_config() -> ConnectionConfig {
    ConnectionConfig {
        options: Default::default(),
        driver: "mongodb".to_string(),
        host: env_or_default("QOREDB_TEST_MONGO_HOST", "127.0.0.1"),
        port: env_u16_or_default("QOREDB_TEST_MONGO_PORT", 27017),
        username: env_or_default("QOREDB_TEST_MONGO_USER", "qoredb"),
        password: env_or_default("QOREDB_TEST_MONGO_PASSWORD", "qoredb_test"),
        database: Some(env_or_default("QOREDB_TEST_MONGO_DB", DEFAULT_DB)),
        ssl: false,
        ssl_mode: None,
        environment: "development".to_string(),
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

fn clickhouse_config() -> ConnectionConfig {
    ConnectionConfig {
        options: Default::default(),
        driver: "clickhouse".to_string(),
        host: env_or_default("QOREDB_TEST_CLICKHOUSE_HOST", "127.0.0.1"),
        port: env_u16_or_default("QOREDB_TEST_CLICKHOUSE_PORT", 8123),
        username: env_or_default("QOREDB_TEST_CLICKHOUSE_USER", "qoredb"),
        // Empty by default: driver refuses Basic-auth over cleartext HTTP.
        // The docker-compose ClickHouse is configured with no password to match.
        password: env_or_default("QOREDB_TEST_CLICKHOUSE_PASSWORD", ""),
        database: Some(env_or_default("QOREDB_TEST_CLICKHOUSE_DB", DEFAULT_DB)),
        ssl: false,
        ssl_mode: None,
        environment: "development".to_string(),
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

fn redis_config() -> ConnectionConfig {
    ConnectionConfig {
        options: Default::default(),
        driver: "redis".to_string(),
        host: env_or_default("QOREDB_TEST_REDIS_HOST", "127.0.0.1"),
        port: env_u16_or_default("QOREDB_TEST_REDIS_PORT", 6379),
        username: env_or_default("QOREDB_TEST_REDIS_USER", "default"),
        password: env_or_default("QOREDB_TEST_REDIS_PASSWORD", "qoredb_test"),
        database: Some(env_or_default("QOREDB_TEST_REDIS_DB", "0")),
        ssl: false,
        ssl_mode: None,
        environment: "development".to_string(),
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

fn elasticsearch_config() -> ConnectionConfig {
    ConnectionConfig {
        options: Default::default(),
        driver: "elasticsearch".to_string(),
        host: env_or_default("QOREDB_TEST_ES_HOST", "127.0.0.1"),
        port: env_u16_or_default("QOREDB_TEST_ES_PORT", 9200),
        username: env_or_default("QOREDB_TEST_ES_USER", ""),
        // Empty by default: the driver refuses to send credentials over
        // cleartext HTTP, and the docker-compose node runs with security off.
        password: env_or_default("QOREDB_TEST_ES_PASSWORD", ""),
        database: None,
        ssl: false,
        ssl_mode: None,
        environment: "development".to_string(),
        read_only: false,
        ssh_tunnel: None,
        pool_acquire_timeout_secs: None,
        pool_max_connections: None,
        pool_min_connections: None,
        proxy: None,
        mssql_auth: None,
        clickhouse_cluster: None,
        search_auth_mode: Some("none".to_string()),
        ssl_ca_cert: None,
    }
}

async fn wait_for_connection<D: DataEngine + ?Sized>(
    driver: &D,
    config: &ConnectionConfig,
) -> EngineResult<()> {
    let mut last_err = None;
    for _ in 0..20 {
        match driver.test_connection(config).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = Some(err);
                sleep(Duration::from_millis(500)).await;
            }
        }
    }

    Err(last_err.unwrap_or_else(|| {
        EngineError::connection_failed("Test connection did not succeed".to_string())
    }))
}

async fn cancel_with_retry<D: DataEngine + ?Sized>(
    driver: &D,
    session: SessionId,
    query_id: QueryId,
) -> EngineResult<()> {
    for _ in 0..10 {
        match driver.cancel(session, Some(query_id)).await {
            Ok(()) => return Ok(()),
            Err(EngineError::ExecutionError { message }) if message.contains("Query not found") => {
                sleep(Duration::from_millis(100)).await;
                continue;
            }
            Err(err) => return Err(err),
        }
    }

    Err(EngineError::execution_error(
        "Query not found after retries",
    ))
}

fn unique_name(prefix: &str) -> String {
    format!("{}_{}", prefix, Uuid::new_v4().simple())
}

/// Walks every page of `table` in `count_mode: none` and checks the guarantees
/// the driver families are supposed to share: a full page announces the next
/// one, the last page does not, no page claims a total, and the over-fetched
/// row never reaches the caller.
async fn assert_count_free_pages<D: DataEngine + ?Sized>(
    driver: &D,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    sort_column: Option<&str>,
    total: usize,
    page_size: u32,
) -> EngineResult<()> {
    let pages = total.div_ceil(page_size as usize);
    let mut seen = 0usize;

    for page in 1..=pages {
        let result = driver
            .query_table(
                session,
                namespace,
                table,
                TableQueryOptions {
                    page: Some(page as u32),
                    page_size: Some(page_size),
                    sort_column: sort_column.map(str::to_string),
                    count_mode: Some(CountMode::None),
                    ..Default::default()
                },
            )
            .await?;

        assert!(
            result.result.rows.len() <= page_size as usize,
            "{table}: page {page} leaked the over-fetched row ({} rows for a page size of {page_size})",
            result.result.rows.len()
        );
        assert_eq!(
            result.total_rows, None,
            "{table}: page {page} reported a total in count-free mode"
        );
        assert_eq!(result.total_rows_source, None);
        assert_eq!(
            result.has_more,
            page < pages,
            "{table}: wrong has_more on page {page} of {pages}"
        );

        seen += result.result.rows.len();
    }

    assert_eq!(
        seen, total,
        "{table}: count-free paging lost or duplicated rows"
    );
    Ok(())
}

fn assert_count(result: &qoredb_lib::engine::types::QueryResult, expected: i64) {
    let value = result
        .rows
        .get(0)
        .and_then(|row| row.values.get(0))
        .cloned()
        .expect("expected a count value");

    match value {
        Value::Int(value) => assert_eq!(value, expected),
        Value::Float(value) => assert_eq!(value as i64, expected),
        other => panic!("unexpected count value: {other:?}"),
    }
}

async fn connect_postgres() -> EngineResult<(Arc<PostgresDriver>, SessionId, ConnectionConfig)> {
    let config = postgres_config();
    let driver = Arc::new(PostgresDriver::new());
    wait_for_connection(driver.as_ref(), &config).await?;
    let session = driver.connect(&config).await?;
    Ok((driver, session, config))
}

async fn connect_mysql() -> EngineResult<(Arc<MySqlDriver>, SessionId, ConnectionConfig)> {
    let config = mysql_config();
    let driver = Arc::new(MySqlDriver::new());
    wait_for_connection(driver.as_ref(), &config).await?;
    let session = driver.connect(&config).await?;
    Ok((driver, session, config))
}

async fn connect_mongo() -> EngineResult<(Arc<MongoDriver>, SessionId, ConnectionConfig)> {
    let config = mongo_config();
    let driver = Arc::new(MongoDriver::new());
    wait_for_connection(driver.as_ref(), &config).await?;
    let session = driver.connect(&config).await?;
    Ok((driver, session, config))
}

async fn connect_redis() -> EngineResult<(Arc<RedisDriver>, SessionId, ConnectionConfig)> {
    let config = redis_config();
    let driver = Arc::new(RedisDriver::new());
    wait_for_connection(driver.as_ref(), &config).await?;
    let session = driver.connect(&config).await?;
    Ok((driver, session, config))
}

async fn connect_elasticsearch()
-> EngineResult<(Arc<ElasticsearchDriver>, SessionId, ConnectionConfig)> {
    let config = elasticsearch_config();
    let driver = Arc::new(ElasticsearchDriver::new());
    wait_for_connection(driver.as_ref(), &config).await?;
    let session = driver.connect(&config).await?;
    Ok((driver, session, config))
}

async fn connect_clickhouse() -> EngineResult<(Arc<ClickHouseDriver>, SessionId, ConnectionConfig)>
{
    let config = clickhouse_config();
    let driver = Arc::new(ClickHouseDriver::new());
    wait_for_connection(driver.as_ref(), &config).await?;
    let session = driver.connect(&config).await?;
    Ok((driver, session, config))
}

#[tokio::test]
async fn postgres_e2e() -> EngineResult<()> {
    let (driver, session, config) = connect_postgres().await?;
    let table = unique_name("qoredb_pg");

    driver
        .execute(
            session,
            &format!(
                "CREATE TABLE IF NOT EXISTS {} (id INT PRIMARY KEY, name TEXT)",
                table
            ),
            QueryId::new(),
        )
        .await?;
    driver
        .execute(session, &format!("DELETE FROM {}", table), QueryId::new())
        .await?;
    driver
        .execute(
            session,
            &format!("INSERT INTO {} (id, name) VALUES (1, 'alpha')", table),
            QueryId::new(),
        )
        .await?;

    let namespaces = driver.list_namespaces(session).await?;
    let db_name = config
        .database
        .clone()
        .unwrap_or_else(|| "postgres".to_string());
    assert!(
        namespaces
            .iter()
            .any(|ns| { ns.database == db_name && ns.schema.as_deref() == Some("public") })
    );

    let namespace = namespaces
        .into_iter()
        .find(|ns| ns.schema.as_deref() == Some("public"))
        .unwrap_or_else(|| Namespace::with_schema(db_name.clone(), "public"));

    let collections = driver
        .list_collections(session, &namespace, CollectionListOptions::default())
        .await?;
    assert!(collections.collections.iter().any(|c| c.name == table));

    let result = driver
        .execute(
            session,
            &format!("SELECT name FROM {} WHERE id = 1", table),
            QueryId::new(),
        )
        .await?;
    assert!(!result.rows.is_empty());

    let cancel_id = QueryId::new();
    let driver_clone = Arc::clone(&driver);
    let handle = tokio::spawn(async move {
        driver_clone
            .execute(session, "SELECT pg_sleep(5)", cancel_id)
            .await
    });

    sleep(Duration::from_millis(200)).await;
    cancel_with_retry(driver.as_ref(), session, cancel_id).await?;

    let exec_result = timeout(Duration::from_secs(6), handle)
        .await
        .map_err(|_| EngineError::execution_error("Cancel did not return in time"))?
        .map_err(|e| EngineError::execution_error(format!("Join error: {}", e)))?;
    assert!(exec_result.is_err());

    driver.begin_transaction(session).await?;
    driver
        .execute(
            session,
            &format!("INSERT INTO {} (id, name) VALUES (2, 'beta')", table),
            QueryId::new(),
        )
        .await?;
    driver.rollback(session).await?;

    let count = driver
        .execute(
            session,
            &format!("SELECT COUNT(*) FROM {}", table),
            QueryId::new(),
        )
        .await?;
    assert_count(&count, 1);

    driver.begin_transaction(session).await?;
    driver
        .execute(
            session,
            &format!("INSERT INTO {} (id, name) VALUES (3, 'gamma')", table),
            QueryId::new(),
        )
        .await?;
    driver.commit(session).await?;

    let count = driver
        .execute(
            session,
            &format!("SELECT COUNT(*) FROM {}", table),
            QueryId::new(),
        )
        .await?;
    assert_count(&count, 2);

    assert_count_free_pages(
        driver.as_ref(),
        session,
        &namespace,
        &table,
        Some("id"),
        2,
        1,
    )
    .await?;

    let _ = driver
        .execute(session, &format!("DROP TABLE {}", table), QueryId::new())
        .await;
    driver.disconnect(session).await?;

    Ok(())
}

#[tokio::test]
async fn mysql_e2e() -> EngineResult<()> {
    let (driver, session, config) = connect_mysql().await?;
    let table = unique_name("qoredb_mysql");

    driver
        .execute(
            session,
            &format!(
                "CREATE TABLE IF NOT EXISTS {} (id INT PRIMARY KEY, name VARCHAR(255))",
                table
            ),
            QueryId::new(),
        )
        .await?;
    driver
        .execute(session, &format!("DELETE FROM {}", table), QueryId::new())
        .await?;
    driver
        .execute(
            session,
            &format!("INSERT INTO {} (id, name) VALUES (1, 'alpha')", table),
            QueryId::new(),
        )
        .await?;

    let namespaces = driver.list_namespaces(session).await?;
    let db_name = config
        .database
        .clone()
        .unwrap_or_else(|| DEFAULT_DB.to_string());
    assert!(namespaces.iter().any(|ns| ns.database == db_name));

    let namespace = Namespace::new(db_name.clone());
    let collections = driver
        .list_collections(session, &namespace, CollectionListOptions::default())
        .await?;
    assert!(collections.collections.iter().any(|c| c.name == table));

    let result = driver
        .execute(
            session,
            &format!("SELECT name FROM {} WHERE id = 1", table),
            QueryId::new(),
        )
        .await?;
    assert!(!result.rows.is_empty());

    let cancel_id = QueryId::new();
    let driver_clone = Arc::clone(&driver);
    let handle = tokio::spawn(async move {
        driver_clone
            .execute(session, "SELECT SLEEP(5)", cancel_id)
            .await
    });

    sleep(Duration::from_millis(200)).await;
    cancel_with_retry(driver.as_ref(), session, cancel_id).await?;

    let exec_result = timeout(Duration::from_secs(6), handle)
        .await
        .map_err(|_| EngineError::execution_error("Cancel did not return in time"))?
        .map_err(|e| EngineError::execution_error(format!("Join error: {}", e)))?;
    match exec_result {
        Ok(res) => {
            // MySQL SLEEP() can return 1 (interrupted) instead of error
            assert!(
                res.execution_time_ms < 4000.0,
                "Query passed but took too long ({}ms), likely not canceled",
                res.execution_time_ms
            );
        }
        Err(_) => {} // Expected error
    }

    driver.begin_transaction(session).await?;
    driver
        .execute(
            session,
            &format!("INSERT INTO {} (id, name) VALUES (2, 'beta')", table),
            QueryId::new(),
        )
        .await?;
    driver.rollback(session).await?;

    let count = driver
        .execute(
            session,
            &format!("SELECT COUNT(*) FROM {}", table),
            QueryId::new(),
        )
        .await?;
    assert_count(&count, 1);

    driver.begin_transaction(session).await?;
    driver
        .execute(
            session,
            &format!("INSERT INTO {} (id, name) VALUES (3, 'gamma')", table),
            QueryId::new(),
        )
        .await?;
    driver.commit(session).await?;

    let count = driver
        .execute(
            session,
            &format!("SELECT COUNT(*) FROM {}", table),
            QueryId::new(),
        )
        .await?;
    assert_count(&count, 2);

    assert_count_free_pages(
        driver.as_ref(),
        session,
        &namespace,
        &table,
        Some("id"),
        2,
        1,
    )
    .await?;

    let _ = driver
        .execute(session, &format!("DROP TABLE {}", table), QueryId::new())
        .await;
    driver.disconnect(session).await?;

    Ok(())
}

#[tokio::test]
async fn mongodb_e2e() -> EngineResult<()> {
    let (driver, session, config) = connect_mongo().await?;
    let db_name = config
        .database
        .clone()
        .unwrap_or_else(|| DEFAULT_DB.to_string());
    let collection = unique_name("qoredb_mongo");

    let data = RowData::new()
        .with_column("name", Value::Text("alpha".to_string()))
        .with_column("value", Value::Int(1));
    let namespace = Namespace::new(db_name.clone());
    driver
        .insert_row(session, &namespace, &collection, &data)
        .await?;
    driver
        .insert_row(
            session,
            &namespace,
            &collection,
            &RowData::new()
                .with_column("name", Value::Text("beta".to_string()))
                .with_column("value", Value::Int(2)),
        )
        .await?;

    let namespaces = driver.list_namespaces(session).await?;
    assert!(namespaces.iter().any(|ns| ns.database == db_name));

    let collections = driver
        .list_collections(session, &namespace, CollectionListOptions::default())
        .await?;
    assert!(collections.collections.iter().any(|c| c.name == collection));

    let query = json!({
        "database": db_name,
        "collection": collection,
        "query": {}
    })
    .to_string();
    let result = driver.execute(session, &query, QueryId::new()).await?;
    assert!(!result.rows.is_empty());

    assert_count_free_pages(
        driver.as_ref(),
        session,
        &namespace,
        &collection,
        Some("value"),
        2,
        1,
    )
    .await?;

    driver.disconnect(session).await?;
    Ok(())
}

#[tokio::test]
async fn redis_e2e() -> EngineResult<()> {
    let (driver, session, _config) = match connect_redis().await {
        Ok(conn) => conn,
        Err(err) if !redis_test_required() && is_service_unavailable(&err) => {
            eprintln!(
                "redis_e2e skipped: Redis is unavailable (set QOREDB_TEST_REDIS_REQUIRED=true to fail instead): {}",
                err
            );
            return Ok(());
        }
        Err(err) => return Err(err),
    };
    let ns0 = Namespace::new("db0");
    let ns1 = Namespace::new("db1");
    let key = unique_name("qoredb_redis_key");
    let stream = unique_name("qoredb_redis_stream");

    driver
        .execute_in_namespace(
            session,
            Some(ns0.clone()),
            &format!("SET {} zero", key),
            QueryId::new(),
        )
        .await?;
    driver
        .execute_in_namespace(
            session,
            Some(ns1.clone()),
            &format!("SET {} one", key),
            QueryId::new(),
        )
        .await?;

    for i in 1..=3 {
        driver
            .execute_in_namespace(
                session,
                Some(ns0.clone()),
                &format!("XADD {} * field value{}", stream, i),
                QueryId::new(),
            )
            .await?;
    }

    let mut handles = Vec::new();
    for _ in 0..20 {
        let d0 = Arc::clone(&driver);
        let k0 = key.clone();
        let n0 = ns0.clone();
        handles.push(tokio::spawn(async move {
            d0.execute_in_namespace(session, Some(n0), &format!("GET {}", k0), QueryId::new())
                .await
        }));

        let d1 = Arc::clone(&driver);
        let k1 = key.clone();
        let n1 = ns1.clone();
        handles.push(tokio::spawn(async move {
            d1.execute_in_namespace(session, Some(n1), &format!("GET {}", k1), QueryId::new())
                .await
        }));
    }

    for (idx, handle) in handles.into_iter().enumerate() {
        let result = handle
            .await
            .map_err(|e| EngineError::execution_error(format!("Join error: {}", e)))??;
        let expected = if idx % 2 == 0 { "zero" } else { "one" };
        match result.rows.first().and_then(|row| row.values.first()) {
            Some(Value::Text(value)) => assert_eq!(value, expected),
            other => panic!("Unexpected GET result: {:?}", other),
        }
    }

    let page1 = driver
        .query_table(
            session,
            &ns0,
            &stream,
            TableQueryOptions {
                page: Some(1),
                page_size: Some(1),
                ..Default::default()
            },
        )
        .await?;
    let page2 = driver
        .query_table(
            session,
            &ns0,
            &stream,
            TableQueryOptions {
                page: Some(2),
                page_size: Some(1),
                ..Default::default()
            },
        )
        .await?;

    let id1 = match page1.result.rows.first().and_then(|row| row.values.first()) {
        Some(Value::Text(id)) => id.clone(),
        other => panic!("Unexpected stream page1 row id: {:?}", other),
    };
    let id2 = match page2.result.rows.first().and_then(|row| row.values.first()) {
        Some(Value::Text(id)) => id.clone(),
        other => panic!("Unexpected stream page2 row id: {:?}", other),
    };
    assert_ne!(
        id1, id2,
        "Stream pagination should return different entry IDs"
    );

    let namespaces = driver.list_namespaces(session).await?;
    assert!(namespaces.iter().any(|ns| ns.database == "db0"));
    assert!(namespaces.iter().any(|ns| ns.database == "db1"));

    let collections = driver
        .list_collections(session, &ns0, CollectionListOptions::default())
        .await?;
    assert!(collections.collections.iter().any(|c| c.name == key));
    assert!(collections.collections.iter().any(|c| c.name == stream));

    let list_key = unique_name("qoredb_redis_list");
    let hash_key = unique_name("qoredb_redis_hash");
    driver
        .execute_in_namespace(
            session,
            Some(ns0.clone()),
            &format!("RPUSH {} a b c", list_key),
            QueryId::new(),
        )
        .await?;
    driver
        .execute_in_namespace(
            session,
            Some(ns0.clone()),
            &format!("HSET {} f1 v1 f2 v2 f3 v3", hash_key),
            QueryId::new(),
        )
        .await?;

    // Two distinct paging paths: LRANGE gives an exact window, HSCAN only
    // approximates one and is the fragile side of the family.
    assert_count_free_pages(driver.as_ref(), session, &ns0, &list_key, None, 3, 2).await?;
    assert_count_free_pages(driver.as_ref(), session, &ns0, &hash_key, None, 3, 2).await?;

    let _ = driver
        .execute_in_namespace(
            session,
            Some(ns0),
            &format!("DEL {} {} {} {}", key, stream, list_key, hash_key),
            QueryId::new(),
        )
        .await;
    let _ = driver
        .execute_in_namespace(session, Some(ns1), &format!("DEL {}", key), QueryId::new())
        .await;
    driver.disconnect(session).await?;
    Ok(())
}

/// Walks a table by cursor and checks the guarantees keyset pagination is
/// supposed to buy: a total order, every row exactly once, and — the point of
/// the whole exercise — rows already seen that do not reappear when the table
/// is written to between two pages.
async fn assert_keyset_pagination<D: DataEngine + ?Sized>(
    driver: &D,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    unique_key: &[&str],
    sort_column: Option<&str>,
    sort_direction: Option<SortDirection>,
    page_size: u32,
    expected_ids: &[i64],
    id_column: &str,
    // Statements run after the first page, to write to the table mid-walk.
    // A slice rather than one string: a prepared statement takes one command.
    disturb: &[String],
) -> EngineResult<Vec<i64>> {
    let keyset: Vec<String> = unique_key.iter().map(|col| col.to_string()).collect();
    let mut cursor: Option<String> = None;
    let mut seen: Vec<i64> = Vec::new();
    let mut pages = 0;

    loop {
        pages += 1;
        assert!(pages < 50, "{table}: keyset walk did not terminate");

        let page = driver
            .query_table(
                session,
                namespace,
                table,
                TableQueryOptions {
                    page_size: Some(page_size),
                    sort_column: sort_column.map(str::to_string),
                    sort_direction,
                    keyset_columns: Some(keyset.clone()),
                    cursor: cursor.clone(),
                    count_mode: Some(CountMode::None),
                    ..Default::default()
                },
            )
            .await?;

        assert_eq!(
            page.pagination_strategy,
            PaginationStrategy::Keyset,
            "{table}: driver fell back to offset despite a unique key"
        );
        assert_eq!(
            page.ordering_guarantee,
            OrderingGuarantee::Stable,
            "{table}: keyset must announce a stable order"
        );
        assert!(
            page.result.rows.len() <= page_size as usize,
            "{table}: the over-fetched row reached the caller"
        );

        let id_index = page
            .result
            .columns
            .iter()
            .position(|col| col.name == id_column)
            .unwrap_or_else(|| panic!("{table}: {id_column} missing from the projection"));
        for row in &page.result.rows {
            match &row.values[id_index] {
                Value::Int(id) => seen.push(*id),
                other => panic!("{table}: unexpected id value {other:?}"),
            }
        }

        if pages == 1 {
            for sql in disturb {
                driver.execute(session, sql, QueryId::new()).await?;
            }
        }

        if !page.has_more {
            assert!(
                page.next_cursor.is_none(),
                "{table}: last page still handed out a cursor"
            );
            break;
        }
        cursor = Some(
            page.next_cursor
                .clone()
                .unwrap_or_else(|| panic!("{table}: has_more without a cursor")),
        );
    }

    let mut unique = seen.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        seen.len(),
        "{table}: keyset returned a row twice: {seen:?}"
    );
    for id in expected_ids {
        assert!(
            seen.contains(id),
            "{table}: row {id} was skipped, saw {seen:?}"
        );
    }

    Ok(seen)
}

/// Keyset pagination end to end on PostgreSQL: simple key, composite key,
/// mixed sort directions, and a table written to between two pages.
#[tokio::test]
async fn postgres_keyset_pagination() -> EngineResult<()> {
    let (driver, session, config) = connect_postgres().await?;
    let table = unique_name("qoredb_keyset");
    let db_name = config
        .database
        .clone()
        .unwrap_or_else(|| "postgres".to_string());
    let namespace = Namespace::with_schema(db_name, "public");

    driver
        .execute(
            session,
            &format!("CREATE TABLE {table} (id INT PRIMARY KEY, bucket INT NOT NULL, label TEXT)"),
            QueryId::new(),
        )
        .await?;
    driver
        .execute(
            session,
            &format!(
                "INSERT INTO {table} (id, bucket, label) VALUES \
                 (1,1,'a'),(2,1,'b'),(3,2,'c'),(4,2,'d'),(5,3,'e'),(6,3,'f'),(7,4,'g')"
            ),
            QueryId::new(),
        )
        .await?;

    let all: Vec<i64> = (1..=7).collect();

    // Simple key, no sort: the primary key alone orders the walk.
    let seen = assert_keyset_pagination(
        driver.as_ref(),
        session,
        &namespace,
        &table,
        &["id"],
        None,
        None,
        2,
        &all,
        "id",
        &[],
    )
    .await?;
    assert_eq!(seen, all, "keyset must preserve the key order");

    // Sort on a non-unique column: the key breaks the ties, so rows in the same
    // bucket cannot be served twice or skipped across the page boundary.
    let seen = assert_keyset_pagination(
        driver.as_ref(),
        session,
        &namespace,
        &table,
        &["id"],
        Some("bucket"),
        Some(SortDirection::Asc),
        2,
        &all,
        "id",
        &[],
    )
    .await?;
    assert_eq!(seen.len(), all.len());

    // Descending: the comparison has to flip with the direction, otherwise the
    // second page walks the wrong way and returns nothing.
    let seen = assert_keyset_pagination(
        driver.as_ref(),
        session,
        &namespace,
        &table,
        &["id"],
        Some("bucket"),
        Some(SortDirection::Desc),
        3,
        &all,
        "id",
        &[],
    )
    .await?;
    assert_eq!(seen.len(), all.len());

    // Composite key.
    let seen = assert_keyset_pagination(
        driver.as_ref(),
        session,
        &namespace,
        &table,
        &["bucket", "id"],
        None,
        None,
        2,
        &all,
        "id",
        &[],
    )
    .await?;
    assert_eq!(seen.len(), all.len());

    // Written to mid-walk: a row inserted before the boundary must not push the
    // window backwards, and a deleted row must not shift the rest into a gap —
    // both of which OFFSET does.
    let disturb = [
        format!("INSERT INTO {table} (id, bucket, label) VALUES (0,0,'z')"),
        format!("DELETE FROM {table} WHERE id = 7"),
    ];
    let seen = assert_keyset_pagination(
        driver.as_ref(),
        session,
        &namespace,
        &table,
        &["id"],
        None,
        None,
        2,
        &[3, 4, 5, 6],
        "id",
        &disturb,
    )
    .await?;
    assert!(
        !seen.contains(&0),
        "a row inserted behind the cursor reappeared: {seen:?}"
    );

    // Regression: the schema lands after the first page, so page 2 can arrive
    // with a unique key but no cursor. Answering that with a keyset drops the
    // offset and serves the first page again — the whole table read as a loop.
    let page_two_without_cursor = driver
        .query_table(
            session,
            &namespace,
            &table,
            TableQueryOptions {
                page: Some(2),
                page_size: Some(3),
                keyset_columns: Some(vec!["id".to_string()]),
                count_mode: Some(CountMode::None),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(
        page_two_without_cursor.pagination_strategy,
        PaginationStrategy::Offset,
        "a cursorless later page must stay on offset"
    );
    let first_id = match &page_two_without_cursor.result.rows[0].values[0] {
        Value::Int(id) => *id,
        other => panic!("unexpected id {other:?}"),
    };
    assert_ne!(first_id, 1, "page 2 restarted from the first row");

    // No unique key declared: the driver must say so rather than pretend.
    let offset_page = driver
        .query_table(
            session,
            &namespace,
            &table,
            TableQueryOptions {
                page_size: Some(2),
                count_mode: Some(CountMode::None),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(offset_page.pagination_strategy, PaginationStrategy::Offset);
    assert_eq!(offset_page.ordering_guarantee, OrderingGuarantee::None);
    assert!(offset_page.next_cursor.is_none());

    let _ = driver
        .execute(session, &format!("DROP TABLE {table}"), QueryId::new())
        .await;
    driver.disconnect(session).await?;
    Ok(())
}

/// The same contract on MySQL, whose driver builds the predicate with `?`
/// placeholders rather than numbered ones.
#[tokio::test]
async fn mysql_keyset_pagination() -> EngineResult<()> {
    let (driver, session, config) = connect_mysql().await?;
    let table = unique_name("qoredb_keyset");
    let namespace = Namespace::new(config.database.clone().unwrap_or_else(|| DEFAULT_DB.into()));

    driver
        .execute(
            session,
            &format!("CREATE TABLE {table} (id INT PRIMARY KEY, bucket INT NOT NULL)"),
            QueryId::new(),
        )
        .await?;
    driver
        .execute(
            session,
            &format!("INSERT INTO {table} (id, bucket) VALUES (1,1),(2,1),(3,2),(4,2),(5,3)"),
            QueryId::new(),
        )
        .await?;

    let all: Vec<i64> = (1..=5).collect();
    let seen = assert_keyset_pagination(
        driver.as_ref(),
        session,
        &namespace,
        &table,
        &["id"],
        Some("bucket"),
        Some(SortDirection::Asc),
        2,
        &all,
        "id",
        &[],
    )
    .await?;
    assert_eq!(seen.len(), all.len());

    let _ = driver
        .execute(session, &format!("DROP TABLE {table}"), QueryId::new())
        .await;
    driver.disconnect(session).await?;
    Ok(())
}

/// Search is the other fragile family: `from + size` is bounded by
/// `max_result_window`, so the over-fetched row has to be clamped at the edge
/// rather than sent to the engine.
#[tokio::test]
async fn elasticsearch_count_free_pagination() -> EngineResult<()> {
    let (driver, session, _config) = match connect_elasticsearch().await {
        Ok(conn) => conn,
        Err(err) if !search_test_required() && is_service_unavailable(&err) => {
            eprintln!(
                "elasticsearch_count_free_pagination skipped: Elasticsearch is unavailable (set QOREDB_TEST_SEARCH_REQUIRED=true to fail instead): {}",
                err
            );
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let index = unique_name("qoredb_es");
    driver
        .execute(
            session,
            &format!(
                "PUT /{index}\n{{\"mappings\":{{\"properties\":{{\"n\":{{\"type\":\"integer\"}}}}}}}}"
            ),
            QueryId::new(),
        )
        .await?;

    let mut bulk = String::new();
    for n in 1..=5 {
        bulk.push_str(&format!(
            "{{\"index\":{{\"_id\":\"{n}\"}}}}\n{{\"n\":{n}}}\n"
        ));
    }
    driver
        .execute(
            session,
            &format!("POST /{index}/_bulk\n{bulk}"),
            QueryId::new(),
        )
        .await?;
    // Indexing is near-real-time: without a refresh the docs are not searchable.
    driver
        .execute(session, &format!("POST /{index}/_refresh"), QueryId::new())
        .await?;

    let namespace = Namespace::new("elasticsearch");
    assert_count_free_pages(
        driver.as_ref(),
        session,
        &namespace,
        &index,
        Some("n"),
        5,
        2,
    )
    .await?;

    let _ = driver
        .execute(session, &format!("DELETE /{index}"), QueryId::new())
        .await;
    driver.disconnect(session).await?;
    Ok(())
}

async fn test_streaming<D: DataEngine + ?Sized>(
    driver: &D,
    session: SessionId,
    query: &str,
    expected_count: u64,
) -> EngineResult<()> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    // Launch streaming in background
    let stream_future = driver.execute_stream(session, query, QueryId::new(), tx);

    let receive_future = async {
        let mut columns_received = false;
        let mut rows_received = 0;
        let mut done_received = false;

        while let Some(event) = rx.recv().await {
            match event {
                qoredb_lib::engine::traits::StreamEvent::Columns(cols) => {
                    assert!(!columns_received, "Columns received twice");
                    assert!(!cols.is_empty(), "Columns should not be empty");
                    columns_received = true;
                }
                qoredb_lib::engine::traits::StreamEvent::Row(_row) => {
                    rows_received += 1;
                }
                qoredb_lib::engine::traits::StreamEvent::RowBatch(batch) => {
                    rows_received += batch.len() as u64;
                }
                qoredb_lib::engine::traits::StreamEvent::Error(e) => {
                    panic!("Stream error: {}", e);
                }
                qoredb_lib::engine::traits::StreamEvent::Done(count) => {
                    assert!(!done_received, "Done received twice");
                    assert_eq!(count, rows_received, "Done count mismatch");
                    done_received = true;
                }
            }
        }

        assert!(columns_received, "Never received columns");
        assert!(done_received, "Never received done signal");
        assert_eq!(rows_received, expected_count, "Row count mismatch");
        Ok::<(), EngineError>(())
    };

    // run both
    let (res_stream, res_receive) = tokio::join!(stream_future, receive_future);

    res_stream?;
    res_receive?;

    Ok(())
}

#[tokio::test]
async fn postgres_streaming() -> EngineResult<()> {
    let (driver, session, _config) = connect_postgres().await?;
    let table = unique_name("qoredb_pg_stream");

    driver
        .execute(
            session,
            &format!("CREATE TABLE IF NOT EXISTS {} (id INT)", table),
            QueryId::new(),
        )
        .await?;
    driver
        .execute(
            session,
            &format!("INSERT INTO {} VALUES (1), (2), (3)", table),
            QueryId::new(),
        )
        .await?;

    test_streaming(
        driver.as_ref(),
        session,
        &format!("SELECT * FROM {}", table),
        3,
    )
    .await?;

    driver
        .execute(session, &format!("DROP TABLE {}", table), QueryId::new())
        .await?;
    driver.disconnect(session).await?;
    Ok(())
}

#[tokio::test]
async fn mysql_streaming() -> EngineResult<()> {
    let (driver, session, _config) = connect_mysql().await?;
    let table = unique_name("qoredb_mysql_stream");

    driver
        .execute(
            session,
            &format!("CREATE TABLE IF NOT EXISTS {} (id INT)", table),
            QueryId::new(),
        )
        .await?;
    driver
        .execute(
            session,
            &format!("INSERT INTO {} VALUES (1), (2), (3)", table),
            QueryId::new(),
        )
        .await?;

    test_streaming(
        driver.as_ref(),
        session,
        &format!("SELECT * FROM {}", table),
        3,
    )
    .await?;

    driver
        .execute(session, &format!("DROP TABLE {}", table), QueryId::new())
        .await?;
    driver.disconnect(session).await?;
    Ok(())
}

#[tokio::test]
async fn mongodb_streaming() -> EngineResult<()> {
    let (driver, session, config) = connect_mongo().await?;
    let db_name = config.database.unwrap_or_else(|| DEFAULT_DB.to_string());
    let collection = unique_name("qoredb_mongo_stream");

    // Insert 3 documents
    for i in 1..=3 {
        let data = RowData::new().with_column("val", Value::Int(i));
        let namespace = Namespace::new(db_name.clone());
        driver
            .insert_row(session, &namespace, &collection, &data)
            .await?;
    }

    let query = json!({
        "database": db_name,
        "collection": collection,
        "query": {}
    })
    .to_string();

    test_streaming(driver.as_ref(), session, &query, 3).await?;

    driver.disconnect(session).await?;
    Ok(())
}

#[tokio::test]
async fn clickhouse_e2e() -> EngineResult<()> {
    let connect_result = connect_clickhouse().await;
    let (driver, session, config) = match connect_result {
        Ok(t) => t,
        Err(err) if is_service_unavailable(&err) => {
            eprintln!("clickhouse not reachable, skipping: {err}");
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let table = unique_name("qoredb_ch");
    let db = config.database.clone().unwrap_or_else(|| "default".into());

    // Plain MergeTree on a small int — exercise create / insert / select.
    driver
        .execute(
            session,
            &format!(
                "CREATE TABLE IF NOT EXISTS {db}.{table} \
                 (id UInt32, name String, ts DateTime DEFAULT now()) \
                 ENGINE = MergeTree ORDER BY id"
            ),
            QueryId::new(),
        )
        .await?;

    driver
        .execute(
            session,
            &format!("INSERT INTO {db}.{table} (id, name) VALUES (1, 'alpha'), (2, 'beta')"),
            QueryId::new(),
        )
        .await?;

    // list_collections sees the new table
    let namespace = Namespace::new(db.clone());
    let collections = driver
        .list_collections(session, &namespace, CollectionListOptions::default())
        .await?;
    assert!(
        collections.collections.iter().any(|c| c.name == table),
        "table {table} not listed; got {:?}",
        collections
            .collections
            .iter()
            .map(|c| &c.name)
            .collect::<Vec<_>>()
    );

    // describe_table exposes columns, types, and the primary-key column
    let schema = driver.describe_table(session, &namespace, &table).await?;
    let col_names: Vec<&str> = schema.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(col_names.contains(&"id"));
    assert!(col_names.contains(&"name"));
    assert!(col_names.contains(&"ts"));
    assert_eq!(schema.primary_key.as_deref(), Some(&["id".to_string()][..]));

    // Read back via execute, then via preview_table.
    let select = driver
        .execute(
            session,
            &format!("SELECT count() FROM {db}.{table}"),
            QueryId::new(),
        )
        .await?;
    assert_count(&select, 2);

    let preview = driver
        .preview_table(session, &namespace, &table, 10)
        .await?;
    assert_eq!(preview.rows.len(), 2);

    // Pagination via query_table.
    let mut opts = TableQueryOptions::default();
    opts.page_size = Some(1);
    opts.page = Some(1);
    opts.sort_column = Some("id".into());
    let paged = driver
        .query_table(session, &namespace, &table, opts)
        .await?;
    assert_eq!(paged.result.rows.len(), 1);
    assert_eq!(paged.total_rows, Some(2));

    assert_count_free_pages(
        driver.as_ref(),
        session,
        &namespace,
        &table,
        Some("id"),
        2,
        1,
    )
    .await?;

    // Cleanup.
    let _ = driver
        .execute(session, &format!("DROP TABLE {db}.{table}"), QueryId::new())
        .await;
    driver.disconnect(session).await?;
    Ok(())
}

/// A value the browser edits must reach the column unchanged. JavaScript reads
/// every integer as a double, so anything past 2^53 and anything a `numeric`
/// carries beyond a double's digits has to travel as text — which only works if
/// the driver can bind text to a non-text column.
#[tokio::test]
async fn postgres_exact_numeric_round_trip() -> EngineResult<()> {
    let (driver, session, config) = connect_postgres().await?;
    let table = unique_name("qoredb_pg_num");
    let db_name = config
        .database
        .clone()
        .unwrap_or_else(|| "postgres".to_string());
    let namespace = Namespace::with_schema(db_name, "public");

    driver
        .execute(
            session,
            &format!(
                "CREATE TABLE {} (id BIGINT PRIMARY KEY, amount NUMERIC(40, 10), tag TEXT)",
                table
            ),
            QueryId::new(),
        )
        .await?;
    driver
        .execute(
            session,
            &format!("INSERT INTO {} (id, amount, tag) VALUES (1, 0, 'x')", table),
            QueryId::new(),
        )
        .await?;

    let big_id: i64 = 9_007_199_254_740_993; // 2^53 + 1
    let exact_amount = "123456789012345678901.1234567890";

    let updated = driver
        .update_row(
            session,
            &namespace,
            &table,
            &RowData::new().with_column("id", Value::Int(1)),
            &RowData::new()
                .with_column("id", Value::Text(big_id.to_string()))
                .with_column("amount", Value::Text(exact_amount.to_string())),
        )
        .await;

    // The engine rejects it: the parameter is typed `text` and the column is
    // not. This is what blocks carrying exact numerics as text on the wire, and
    // therefore what an eventual `Value` change has to solve — a driver cannot
    // disambiguate a digit string bound for a numeric column from one bound for
    // a text column holding digits.
    let error = updated.expect_err("binding text to a numeric column should be refused");
    let message = error.to_string().to_lowercase();
    assert!(
        message.contains("numeric") && message.contains("text"),
        "expected a parameter type mismatch, got: {error}"
    );

    let _ = driver
        .execute(session, &format!("DROP TABLE {}", table), QueryId::new())
        .await;
    driver.disconnect(session).await?;
    Ok(())
}

fn embedded_config(driver: &str, path: &std::path::Path) -> ConnectionConfig {
    ConnectionConfig {
        options: Default::default(),
        driver: driver.to_string(),
        host: path.to_string_lossy().to_string(),
        port: 0,
        username: String::new(),
        password: String::new(),
        database: None,
        ssl: false,
        ssl_mode: None,
        environment: "development".to_string(),
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

/// Temporary database file for an embedded engine. Named after the test so a
/// leftover from a crashed run is identifiable.
fn embedded_path(prefix: &str, extension: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("{}.{}", unique_name(prefix), extension))
}

async fn seed_embedded_rows<D: DataEngine + ?Sized>(
    driver: &D,
    session: SessionId,
    table: &str,
    rows: &[(i64, &str, Option<&str>)],
) -> EngineResult<()> {
    for (id, bucket, label) in rows {
        let label = match label {
            Some(value) => format!("'{value}'"),
            None => "NULL".to_string(),
        };
        driver
            .execute(
                session,
                &format!(
                    "INSERT INTO {table} (id, bucket, label) VALUES ({id}, '{bucket}', {label})"
                ),
                QueryId::new(),
            )
            .await?;
    }
    Ok(())
}

/// Rows whose sort column repeats, so the unique tie-breaker is what makes the
/// walk correct rather than merely plausible.
const EMBEDDED_ROWS: &[(i64, &str, Option<&str>)] = &[
    (1, "a", Some("one")),
    (2, "a", Some("two")),
    (3, "a", None),
    (4, "b", Some("four")),
    (5, "b", Some("five")),
    (6, "b", None),
    (7, "c", Some("seven")),
];

/// SQLite declares `keyset: true`. Nothing had ever executed the predicate it
/// builds — a wrong one returns plausible rows, not an error.
#[tokio::test]
async fn sqlite_keyset_pagination() -> EngineResult<()> {
    let path = embedded_path("qoredb_sqlite", "db");
    let config = embedded_config("sqlite", &path);
    let driver = SqliteDriver::new();
    let session = driver.connect(&config).await?;

    let namespaces = driver.list_namespaces(session).await?;
    let namespace = namespaces
        .into_iter()
        .next()
        .expect("sqlite exposes one namespace per file");

    let table = "walk";
    driver
        .execute(
            session,
            &format!(
                "CREATE TABLE {table} (id INTEGER PRIMARY KEY, bucket TEXT NOT NULL, label TEXT)"
            ),
            QueryId::new(),
        )
        .await?;
    seed_embedded_rows(&driver, session, table, EMBEDDED_ROWS).await?;

    let all: Vec<i64> = (1..=7).collect();
    assert_keyset_pagination(
        &driver,
        session,
        &namespace,
        table,
        &["id"],
        None,
        None,
        3,
        &all,
        "id",
        &[],
    )
    .await?;

    // Sorting on a repeated column: without the tie-breaker the walk would skip
    // or repeat inside a bucket.
    assert_keyset_pagination(
        &driver,
        session,
        &namespace,
        table,
        &["id"],
        Some("bucket"),
        Some(SortDirection::Asc),
        2,
        &all,
        "id",
        &[],
    )
    .await?;

    let descending: Vec<i64> = (1..=7).rev().collect();
    assert_keyset_pagination(
        &driver,
        session,
        &namespace,
        table,
        &["id"],
        Some("bucket"),
        Some(SortDirection::Desc),
        2,
        &descending,
        "id",
        &[],
    )
    .await?;

    driver.disconnect(session).await?;
    let _ = std::fs::remove_file(&path);
    Ok(())
}

/// DuckDB, same contract. MotherDuck is a thin wrapper over this driver, so a
/// green walk here covers both declarations.
#[tokio::test]
async fn duckdb_keyset_pagination() -> EngineResult<()> {
    let path = embedded_path("qoredb_duckdb", "duckdb");
    let config = embedded_config("duckdb", &path);
    let driver = DuckDbDriver::new();
    let session = driver.connect(&config).await?;

    let namespaces = driver.list_namespaces(session).await?;
    let namespace = namespaces
        .into_iter()
        .next()
        .expect("duckdb exposes at least one namespace");

    let table = "walk";
    driver
        .execute(
            session,
            &format!("CREATE TABLE {table} (id BIGINT PRIMARY KEY, bucket VARCHAR NOT NULL, label VARCHAR)"),
            QueryId::new(),
        )
        .await?;
    seed_embedded_rows(&driver, session, table, EMBEDDED_ROWS).await?;

    let all: Vec<i64> = (1..=7).collect();
    assert_keyset_pagination(
        &driver,
        session,
        &namespace,
        table,
        &["id"],
        None,
        None,
        3,
        &all,
        "id",
        &[],
    )
    .await?;

    assert_keyset_pagination(
        &driver,
        session,
        &namespace,
        table,
        &["id"],
        Some("bucket"),
        Some(SortDirection::Asc),
        2,
        &all,
        "id",
        &[],
    )
    .await?;

    driver.disconnect(session).await?;
    let _ = std::fs::remove_file(&path);
    Ok(())
}

/// A large identifier reaches the interface intact, and comes back intact.
///
/// Before the envelope, a `BIGINT` past 2^53 arrived rounded — `JSON.parse`
/// cannot represent it — and delete built its `WHERE` from that rounded value,
/// which removed a neighbouring row while reporting success. This walks the
/// whole round trip on the two rows that collapse onto the same double.
#[tokio::test]
async fn postgres_large_bigint_key_targets_its_own_row() -> EngineResult<()> {
    let (driver, session, config) = connect_postgres().await?;
    let table = unique_name("qoredb_pg_bigkey");
    let db_name = config
        .database
        .clone()
        .unwrap_or_else(|| "postgres".to_string());
    let namespace = Namespace::with_schema(db_name, "public");

    // 2^53 is the last integer a double represents exactly; 2^53 + 1 is not,
    // and rounds to 2^53.
    let exact: i64 = 9_007_199_254_740_992;
    let unrepresentable: i64 = 9_007_199_254_740_993;

    // The wire form carries the digits, so nothing downstream can round it.
    let wire = serde_json::to_string(&Value::Int(unrepresentable)).unwrap();
    assert_eq!(wire, r#"{"$qoreInt":"9007199254740993"}"#);
    let round_tripped: Value = serde_json::from_str(&wire).unwrap();
    assert!(matches!(round_tripped, Value::Int(v) if v == unrepresentable));

    // Both ids sit outside the safe range — 2^53 is representable but not safe,
    // since 2^53 + 1 collapses onto it — so both travel in an envelope. An
    // ordinary integer stays a plain number.
    assert_eq!(
        serde_json::to_string(&Value::Int(exact)).unwrap(),
        r#"{"$qoreInt":"9007199254740992"}"#
    );
    assert_eq!(serde_json::to_string(&Value::Int(42)).unwrap(), "42");

    driver
        .execute(
            session,
            &format!("CREATE TABLE {table} (id BIGINT PRIMARY KEY, label TEXT NOT NULL)"),
            QueryId::new(),
        )
        .await?;
    driver
        .execute(
            session,
            &format!(
                "INSERT INTO {table} (id, label) VALUES ({exact}, 'neighbour'), ({unrepresentable}, 'selected')"
            ),
            QueryId::new(),
        )
        .await?;

    // The user selects the row labelled 'selected'. Its key survives the trip,
    // so this is the exact id the delete carries.
    let key = RowData::new().with_column("id", round_tripped);
    let deleted = driver.delete_row(session, &namespace, &table, &key).await?;
    assert_eq!(deleted.affected_rows, Some(1));

    let remaining = driver
        .execute(
            session,
            &format!("SELECT label FROM {table} ORDER BY id"),
            QueryId::new(),
        )
        .await?;
    let labels: Vec<String> = remaining
        .rows
        .iter()
        .filter_map(|row| row.values[0].as_text().map(str::to_string))
        .collect();

    assert_eq!(
        labels,
        vec!["neighbour".to_string()],
        "the delete must remove the row the user selected, not the one next to it"
    );

    let _ = driver
        .execute(session, &format!("DROP TABLE {}", table), QueryId::new())
        .await;
    driver.disconnect(session).await?;
    Ok(())
}
