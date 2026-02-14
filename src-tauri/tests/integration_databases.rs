use qoredb_lib::engine::{
    drivers::{mongodb::MongoDriver, mysql::MySqlDriver, postgres::PostgresDriver, redis::RedisDriver},
    error::{EngineError, EngineResult},
    traits::DataEngine,
    types::{CollectionListOptions, ConnectionConfig, Namespace, QueryId, RowData, SessionId, TableQueryOptions, Value},
};
use serde_json::json;
use std::sync::Arc;
use tokio::time::{sleep, timeout, Duration};
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

fn is_redis_unavailable(err: &EngineError) -> bool {
    match err {
        EngineError::ConnectionFailed { message }
        | EngineError::ExecutionError { message } => {
            let lower = message.to_ascii_lowercase();
            lower.contains("connection refused")
                || lower.contains("no route to host")
                || lower.contains("timed out")
                || lower.contains("network is unreachable")
                || lower.contains("cannot assign requested address")
        }
        _ => false,
    }
}

fn postgres_config() -> ConnectionConfig {
    ConnectionConfig {
        driver: "postgres".to_string(),
        host: env_or_default("QOREDB_TEST_PG_HOST", "127.0.0.1"),
        port: env_u16_or_default("QOREDB_TEST_PG_PORT", 54321),
        username: env_or_default("QOREDB_TEST_PG_USER", "qoredb"),
        password: env_or_default("QOREDB_TEST_PG_PASSWORD", "qoredb_test"),
        database: Some(env_or_default("QOREDB_TEST_PG_DB", DEFAULT_DB)),
        ssl: false,
        environment: "development".to_string(),
        read_only: false,
        ssh_tunnel: None,
        pool_acquire_timeout_secs: None,
        pool_max_connections: None,
        pool_min_connections: None
    }
}

fn mysql_config() -> ConnectionConfig {
    ConnectionConfig {
        driver: "mysql".to_string(),
        host: env_or_default("QOREDB_TEST_MYSQL_HOST", "127.0.0.1"),
        port: env_u16_or_default("QOREDB_TEST_MYSQL_PORT", 3306),
        username: env_or_default("QOREDB_TEST_MYSQL_USER", "qoredb"),
        password: env_or_default("QOREDB_TEST_MYSQL_PASSWORD", "qoredb_test"),
        database: Some(env_or_default("QOREDB_TEST_MYSQL_DB", DEFAULT_DB)),
        ssl: false,
        environment: "development".to_string(),
        read_only: false,
        ssh_tunnel: None,
        pool_acquire_timeout_secs: None,
        pool_max_connections: None,
        pool_min_connections: None
    }
}

fn mongo_config() -> ConnectionConfig {
    ConnectionConfig {
        driver: "mongodb".to_string(),
        host: env_or_default("QOREDB_TEST_MONGO_HOST", "127.0.0.1"),
        port: env_u16_or_default("QOREDB_TEST_MONGO_PORT", 27017),
        username: env_or_default("QOREDB_TEST_MONGO_USER", "qoredb"),
        password: env_or_default("QOREDB_TEST_MONGO_PASSWORD", "qoredb_test"),
        database: Some(env_or_default("QOREDB_TEST_MONGO_DB", DEFAULT_DB)),
        ssl: false,
        environment: "development".to_string(),
        read_only: false,
        ssh_tunnel: None,
        pool_acquire_timeout_secs: None,
        pool_max_connections: None,
        pool_min_connections: None
    }
}

fn redis_config() -> ConnectionConfig {
    ConnectionConfig {
        driver: "redis".to_string(),
        host: env_or_default("QOREDB_TEST_REDIS_HOST", "127.0.0.1"),
        port: env_u16_or_default("QOREDB_TEST_REDIS_PORT", 6379),
        username: env_or_default("QOREDB_TEST_REDIS_USER", "default"),
        password: env_or_default("QOREDB_TEST_REDIS_PASSWORD", "qoredb_test"),
        database: Some(env_or_default("QOREDB_TEST_REDIS_DB", "0")),
        ssl: false,
        environment: "development".to_string(),
        read_only: false,
        ssh_tunnel: None,
        pool_acquire_timeout_secs: None,
        pool_max_connections: None,
        pool_min_connections: None,
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
            Err(EngineError::ExecutionError { message })
                if message.contains("Query not found") =>
            {
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

#[tokio::test]
async fn postgres_e2e() -> EngineResult<()> {
    let (driver, session, config) = connect_postgres().await?;
    let table = unique_name("qoredb_pg");

    driver
        .execute(
            session,
            &format!("CREATE TABLE IF NOT EXISTS {} (id INT PRIMARY KEY, name TEXT)", table),
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
    let db_name = config.database.clone().unwrap_or_else(|| "postgres".to_string());
    assert!(namespaces.iter().any(|ns| {
        ns.database == db_name && ns.schema.as_deref() == Some("public")
    }));

    let namespace = namespaces
        .into_iter()
        .find(|ns| ns.schema.as_deref() == Some("public"))
        .unwrap_or_else(|| Namespace::with_schema(db_name.clone(), "public"));

    let collections = driver.list_collections(session, &namespace, CollectionListOptions::default()).await?;
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
    let db_name = config.database.clone().unwrap_or_else(|| DEFAULT_DB.to_string());
    assert!(namespaces.iter().any(|ns| ns.database == db_name));

    let namespace = Namespace::new(db_name.clone());
    let collections = driver.list_collections(session, &namespace, CollectionListOptions::default()).await?;
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

    let _ = driver
        .execute(session, &format!("DROP TABLE {}", table), QueryId::new())
        .await;
    driver.disconnect(session).await?;

    Ok(())
}

#[tokio::test]
async fn mongodb_e2e() -> EngineResult<()> {
    let (driver, session, config) = connect_mongo().await?;
    let db_name = config.database.clone().unwrap_or_else(|| DEFAULT_DB.to_string());
    let collection = unique_name("qoredb_mongo");

    let data = RowData::new()
        .with_column("name", Value::Text("alpha".to_string()))
        .with_column("value", Value::Int(1));
    let namespace = Namespace::new(db_name.clone());
    driver
        .insert_row(session, &namespace, &collection, &data)
        .await?;

    let namespaces = driver.list_namespaces(session).await?;
    assert!(namespaces.iter().any(|ns| ns.database == db_name));

    let collections = driver.list_collections(session, &namespace, CollectionListOptions::default()).await?;
    assert!(collections.collections.iter().any(|c| c.name == collection));

    let query = json!({
        "database": db_name,
        "collection": collection,
        "query": {}
    })
    .to_string();
    let result = driver.execute(session, &query, QueryId::new()).await?;
    assert!(!result.rows.is_empty());

    driver.disconnect(session).await?;
    Ok(())
}

#[tokio::test]
async fn redis_e2e() -> EngineResult<()> {
    let (driver, session, _config) = match connect_redis().await {
        Ok(conn) => conn,
        Err(err) if !redis_test_required() && is_redis_unavailable(&err) => {
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
            d0.execute_in_namespace(
                session,
                Some(n0),
                &format!("GET {}", k0),
                QueryId::new(),
            )
            .await
        }));

        let d1 = Arc::clone(&driver);
        let k1 = key.clone();
        let n1 = ns1.clone();
        handles.push(tokio::spawn(async move {
            d1.execute_in_namespace(
                session,
                Some(n1),
                &format!("GET {}", k1),
                QueryId::new(),
            )
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
    assert_ne!(id1, id2, "Stream pagination should return different entry IDs");

    let namespaces = driver.list_namespaces(session).await?;
    assert!(namespaces.iter().any(|ns| ns.database == "db0"));
    assert!(namespaces.iter().any(|ns| ns.database == "db1"));

    let collections =
        driver.list_collections(session, &ns0, CollectionListOptions::default()).await?;
    assert!(collections.collections.iter().any(|c| c.name == key));
    assert!(collections.collections.iter().any(|c| c.name == stream));

    let _ = driver
        .execute_in_namespace(
            session,
            Some(ns0),
            &format!("DEL {} {}", key, stream),
            QueryId::new(),
        )
        .await;
    let _ = driver
        .execute_in_namespace(session, Some(ns1), &format!("DEL {}", key), QueryId::new())
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

    driver.execute(session, &format!("CREATE TABLE IF NOT EXISTS {} (id INT)", table), QueryId::new()).await?;
    driver.execute(session, &format!("INSERT INTO {} VALUES (1), (2), (3)", table), QueryId::new()).await?;

    test_streaming(driver.as_ref(), session, &format!("SELECT * FROM {}", table), 3).await?;

    driver.execute(session, &format!("DROP TABLE {}", table), QueryId::new()).await?;
    driver.disconnect(session).await?;
    Ok(())
}

#[tokio::test]
async fn mysql_streaming() -> EngineResult<()> {
    let (driver, session, _config) = connect_mysql().await?;
    let table = unique_name("qoredb_mysql_stream");

    driver.execute(session, &format!("CREATE TABLE IF NOT EXISTS {} (id INT)", table), QueryId::new()).await?;
    driver.execute(session, &format!("INSERT INTO {} VALUES (1), (2), (3)", table), QueryId::new()).await?;

    test_streaming(driver.as_ref(), session, &format!("SELECT * FROM {}", table), 3).await?;

    driver.execute(session, &format!("DROP TABLE {}", table), QueryId::new()).await?;
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
        driver.insert_row(session, &namespace, &collection, &data).await?;
    }

    let query = json!({
        "database": db_name,
        "collection": collection,
        "query": {}
    }).to_string();

    test_streaming(driver.as_ref(), session, &query, 3).await?;

    driver.disconnect(session).await?;
    Ok(())
}
