use qoredb_lib::engine::{
    drivers::{mongodb::MongoDriver, mysql::MySqlDriver, postgres::PostgresDriver},
    error::{EngineError, EngineResult},
    traits::DataEngine,
    types::{ConnectionConfig, Namespace, QueryId, RowData, SessionId, Value},
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

    let collections = driver.list_collections(session, &namespace).await?;
    assert!(collections.iter().any(|c| c.name == table));

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
    let collections = driver.list_collections(session, &namespace).await?;
    assert!(collections.iter().any(|c| c.name == table));

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

    let collections = driver.list_collections(session, &namespace).await?;
    assert!(collections.iter().any(|c| c.name == collection));

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
