// SPDX-License-Identifier: Apache-2.0

use qore_core::{ConnectionConfig, SessionId, StreamEvent};
use qore_service::ServiceContext;

#[tokio::main]
async fn main() {
    let ctx = ServiceContext::new();

    let path = std::env::temp_dir()
        .join(format!("qore-headless-{}.db", std::process::id()))
        .to_string_lossy()
        .into_owned();

    let config = ConnectionConfig {
        driver: "sqlite".into(),
        host: path.clone(),
        port: 0,
        username: String::new(),
        password: String::new(),
        database: None,
        ssl: false,
        ssl_mode: None,
        environment: "development".into(),
        read_only: false,
        pool_max_connections: None,
        pool_min_connections: None,
        pool_acquire_timeout_secs: None,
        ssh_tunnel: None,
        proxy: None,
        mssql_auth: None,
        clickhouse_cluster: None,
        search_auth_mode: None,
    };

    let session = qore_service::connection::connect(&ctx.session_manager, config)
        .await
        .expect("connect");
    println!("connected (no Tauri): {session:?}");

    for q in [
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
        "INSERT INTO t (name) VALUES ('alice'), ('bob'), ('carol')",
    ] {
        println!("> {q}");
        run(&ctx, session, q, false).await.expect("mutation");
    }

    println!("> SELECT * FROM t");
    run(&ctx, session, "SELECT * FROM t", true)
        .await
        .expect("select");

    let _ = qore_service::connection::disconnect(
        &ctx.session_manager,
        &ctx.query_rate_limiter,
        session,
    )
    .await;
    let _ = std::fs::remove_file(&path);
    println!("done");
}

async fn run(
    ctx: &ServiceContext,
    session: SessionId,
    query: &str,
    stream: bool,
) -> Result<(), String> {
    let session_id = session.0.to_string();
    let pf = qore_service::query::preflight(
        &ctx.session_manager,
        &ctx.query_rate_limiter,
        &ctx.interceptor,
        &ctx.policy,
        session,
        &session_id,
        query,
        None,
        false,
    )
    .await?;

    let query_id = ctx.query_manager.register(session).await;
    let should_stream = stream && pf.driver.capabilities().streaming;

    let stream_sender = if should_stream {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    StreamEvent::Columns(c) => println!("  [stream] columns: {}", c.len()),
                    StreamEvent::Row(_) => println!("  [stream] row"),
                    StreamEvent::RowBatch(b) => println!("  [stream] batch: {} rows", b.len()),
                    StreamEvent::Done(n) => println!("  [stream] done: {n} rows"),
                    StreamEvent::Error(e) => println!("  [stream] error: {e}"),
                }
            }
        });
        Some(tx)
    } else {
        None
    };

    let outcome = qore_service::query::execute(
        &ctx.query_manager,
        &ctx.query_cache,
        &ctx.interceptor,
        &ctx.policy,
        pf.driver,
        &pf.context,
        session,
        None,
        query,
        query_id,
        pf.is_mutation,
        pf.connection_key.as_deref(),
        pf.safety_warning.as_deref(),
        Some(30_000),
        false,
        None,
        stream_sender,
        |_, _| {},
    )
    .await;

    if let Some(err) = outcome.error {
        return Err(err);
    }
    match outcome.result {
        Some(r) => println!(
            "  -> {} cols, {} rows, affected={:?}",
            r.columns.len(),
            r.rows.len(),
            r.affected_rows
        ),
        None => println!("  -> ok (streamed or no tabular result)"),
    }
    Ok(())
}
