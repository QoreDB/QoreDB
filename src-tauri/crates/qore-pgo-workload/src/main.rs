// SPDX-License-Identifier: Apache-2.0

//! Headless PGO workload for QoreDB.
//!
//! Drives the SQLite driver through `DataEngine` so the LLVM PGO pass records
//! profiles for the hot paths the production app spends most CPU cycles in:
//!   - SQLx Sqlite pool acquire / fetch / row decoding
//!   - `qore-drivers` row → `Value` conversion (`extract_value`)
//!   - `serde_json` serialization of the universal `Value` enum
//!   - `csv::Writer` row emission
//!
//! Run with `RUSTFLAGS="-Cprofile-generate=$DIR"`. The pgo-release workflow
//! merges the resulting `.profraw` files via `llvm-profdata` and feeds the
//! merged profile into the second-stage release build of `qoredb`.
//!
//! NOT shipped — workspace-only binary.

use std::error::Error;
use std::fmt::Write as _;
use std::sync::Arc;

use futures::future::join_all;
use qore_core::{ConnectionConfig, DataEngine, QueryId, SessionId, StreamEvent, Value};
use qore_drivers::drivers::sqlite::SqliteDriver;
use tokio::sync::mpsc;

const ROW_COUNT: usize = 50_000;
const BATCH_SIZE: usize = 250;
const SELECT_ITERATIONS: usize = 3;
const PARALLEL_WORKERS: usize = 4;

type WorkloadResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> WorkloadResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!(
        rows = ROW_COUNT,
        iterations = SELECT_ITERATIONS,
        "starting PGO workload"
    );

    let driver: Arc<SqliteDriver> = Arc::new(SqliteDriver::new());
    let session = driver.connect(&sqlite_config()).await?;

    seed_table(driver.as_ref(), session).await?;

    let queries: &[&str] = &[
        "SELECT id, name, age, score, payload, blob FROM users ORDER BY id",
        "SELECT id, name FROM users WHERE age BETWEEN 25 AND 60 ORDER BY name LIMIT 10000",
        "SELECT age, COUNT(*) AS cnt, AVG(score) AS avg_score \
         FROM users GROUP BY age ORDER BY age",
        "SELECT u.id, u.name, friends.name AS friend FROM users u \
         JOIN users friends ON friends.id = (u.id % 100) + 1 LIMIT 20000",
    ];

    for iteration in 0..SELECT_ITERATIONS {
        for sql in queries {
            stream_and_export(driver.clone(), session, sql).await?;
        }
        tracing::info!(iteration, "completed select pass");
    }

    let parallel: Vec<_> = (0..PARALLEL_WORKERS)
        .map(|i| {
            let driver = driver.clone();
            let sql = format!(
                "SELECT id, name, score FROM users WHERE id % {workers} = {i} ORDER BY id",
                workers = PARALLEL_WORKERS,
                i = i,
            );
            tokio::spawn(async move { stream_and_export(driver, session, &sql).await })
        })
        .collect();
    for r in join_all(parallel).await {
        r??;
    }

    driver.disconnect(session).await?;
    tracing::info!("PGO workload done");
    Ok(())
}

fn sqlite_config() -> ConnectionConfig {
    ConnectionConfig {
        driver: "sqlite".into(),
        host: ":memory:".into(),
        port: 0,
        username: String::new(),
        password: String::new(),
        database: None,
        ssl: false,
        ssl_mode: None,
        environment: "development".into(),
        read_only: false,
        pool_max_connections: Some(4),
        pool_min_connections: Some(1),
        pool_acquire_timeout_secs: Some(15),
        ssh_tunnel: None,
        proxy: None,
        mssql_auth: None,
    }
}

async fn seed_table(driver: &SqliteDriver, session: SessionId) -> WorkloadResult<()> {
    driver
        .execute(
            session,
            "CREATE TABLE IF NOT EXISTS users (\
                id INTEGER PRIMARY KEY, \
                name TEXT, \
                age INTEGER, \
                score REAL, \
                payload TEXT, \
                blob BLOB)",
            QueryId::new(),
        )
        .await?;

    let batches = ROW_COUNT / BATCH_SIZE;
    let mut sql = String::with_capacity(64 + BATCH_SIZE * 96);
    for b in 0..batches {
        sql.clear();
        sql.push_str("INSERT INTO users (id, name, age, score, payload, blob) VALUES ");
        for i in 0..BATCH_SIZE {
            if i > 0 {
                sql.push(',');
            }
            let id = b * BATCH_SIZE + i + 1;
            let age = 18 + (id % 60);
            let score = (id as f64) * 0.5;
            write!(
                &mut sql,
                "({id}, 'user_{id}', {age}, {score}, '{{\"id\":{id},\"tag\":\"row\"}}', x'deadbeef')"
            )?;
        }
        driver.execute(session, &sql, QueryId::new()).await?;
    }
    tracing::info!(rows = ROW_COUNT, "seeded sqlite table");
    Ok(())
}

async fn stream_and_export(
    driver: Arc<SqliteDriver>,
    session: SessionId,
    sql: &str,
) -> WorkloadResult<()> {
    let (tx, mut rx) = mpsc::channel::<StreamEvent>(256);
    let driver_clone = driver.clone();
    let sql_owned = sql.to_string();
    let producer = tokio::spawn(async move {
        driver_clone
            .execute_stream(session, &sql_owned, QueryId::new(), tx)
            .await
    });

    let mut json_buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut csv_writer = csv::Writer::from_writer(Vec::with_capacity(64 * 1024));
    let mut row_count: u64 = 0;
    let mut header_written = false;

    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::Columns(cols) => {
                if !header_written {
                    let header: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
                    csv_writer.write_record(header)?;
                    header_written = true;
                }
            }
            StreamEvent::Row(row) => {
                emit_row(&mut json_buf, &mut csv_writer, &row.values)?;
                row_count += 1;
            }
            StreamEvent::RowBatch(rows) => {
                for row in rows {
                    emit_row(&mut json_buf, &mut csv_writer, &row.values)?;
                    row_count += 1;
                }
            }
            StreamEvent::Done(_) => break,
            StreamEvent::Error(e) => {
                return Err(format!("stream error: {}", e).into());
            }
        }
    }

    csv_writer.flush()?;
    let _ = csv_writer.into_inner().map_err(|e| e.to_string())?;

    producer.await??;

    tracing::debug!(rows = row_count, "stream done");
    Ok(())
}

fn emit_row(
    json_buf: &mut Vec<u8>,
    csv_writer: &mut csv::Writer<Vec<u8>>,
    values: &[Value],
) -> WorkloadResult<()> {
    json_buf.clear();
    serde_json::to_writer(&mut *json_buf, values)?;

    let record: Vec<String> = values
        .iter()
        .map(|v| match v {
            Value::Null => String::new(),
            Value::Text(s) => s.clone(),
            other => serde_json::to_string(other).unwrap_or_default(),
        })
        .collect();
    csv_writer.write_record(record)?;
    Ok(())
}
