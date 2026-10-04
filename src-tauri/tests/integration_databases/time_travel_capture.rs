// SPDX-License-Identifier: BUSL-1.1

use super::*;
use qore_core::DriverRegistry;
use qore_drivers::session_manager::SessionManager;
use qore_service::{
    cache::QueryCache,
    interceptor::InterceptorPipeline,
    mutation::{
        batch::apply_batch_with_capture,
        capture::{finish_capture, prepare_capture},
    },
};
use qore_sql::generator::{SandboxChangeDto, SandboxChangeType};

#[tokio::test]
async fn postgres_generated_insert_capture_reads_final_trigger_image() -> EngineResult<()> {
    let Some((driver, session, config)) = connect_or_skip(
        connect_postgres().await,
        Service::Postgres,
        "postgres_generated_insert_capture_reads_final_trigger_image",
    )?
    else {
        return Ok(());
    };
    let schema = unique_name("capture");
    let namespace = Namespace {
        database: config.database.unwrap(),
        schema: Some(schema.clone()),
    };
    for sql in [
        format!("CREATE SCHEMA {schema}"),
        format!(
            "CREATE TABLE {schema}.items (\"tenant\"\"key\" TEXT DEFAULT 'tenant', id BIGINT GENERATED ALWAYS AS IDENTITY (START WITH 9007199254740993), label TEXT DEFAULT 'initial', PRIMARY KEY (\"tenant\"\"key\", id))"
        ),
        format!(
            "CREATE FUNCTION {schema}.normalize() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE {schema}.items SET label = upper(NEW.label) WHERE id = NEW.id; RETURN NEW; END $$"
        ),
        format!(
            "CREATE TRIGGER normalize AFTER INSERT ON {schema}.items FOR EACH ROW EXECUTE FUNCTION {schema}.normalize()"
        ),
    ] {
        driver.execute(session, &sql, QueryId::new()).await?;
    }
    let data = RowData::new();
    let mut prepared = prepare_capture(
        driver.as_ref(),
        session,
        &namespace,
        "items",
        SandboxChangeType::Insert,
        &data,
    )
    .await;
    let outcome = driver
        .insert_row_returning(
            session,
            &namespace,
            "items",
            &data,
            prepared.returning_columns(),
        )
        .await?;
    assert_eq!(outcome.result.affected_rows, Some(1));
    assert!(outcome.result.rows.is_empty());
    assert!(outcome.result.columns.is_empty());
    let returned = outcome.returned_values.as_ref().unwrap();
    assert_eq!(
        returned.columns.len(),
        2,
        "only declared primary-key columns"
    );
    prepared.use_inserted_values(outcome.returned_values);
    let capture = finish_capture(
        driver.as_ref(),
        session,
        &namespace,
        "items",
        SandboxChangeType::Insert,
        &data,
        prepared,
        &outcome.result,
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(&capture.primary_key.columns["id"]).unwrap(),
        serde_json::to_value(Value::Int(9007199254740993)).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&capture.primary_key.columns["tenant\"key"]).unwrap(),
        "tenant"
    );
    assert_eq!(
        serde_json::to_value(&capture.after.unwrap().columns["label"]).unwrap(),
        "INITIAL"
    );
    driver
        .execute(
            session,
            &format!("DROP SCHEMA {schema} CASCADE"),
            QueryId::new(),
        )
        .await?;
    driver.disconnect(session).await
}

#[tokio::test]
async fn postgres_generated_batch_capture_commits_or_discards() -> EngineResult<()> {
    let Some((driver, initial_session, config)) = connect_or_skip(
        connect_postgres().await,
        Service::Postgres,
        "postgres_generated_batch_capture_commits_or_discards",
    )?
    else {
        return Ok(());
    };
    driver.disconnect(initial_session).await?;
    let mut registry = DriverRegistry::new();
    registry.register(driver.clone());
    let manager = SessionManager::new(Arc::new(registry));
    let session = manager.connect(config.clone()).await?;
    let schema = unique_name("batch_capture");
    let namespace = Namespace {
        database: config.database.unwrap(),
        schema: Some(schema.clone()),
    };
    driver
        .execute(session, &format!("CREATE SCHEMA {schema}"), QueryId::new())
        .await?;
    driver.execute(session, &format!("CREATE TABLE {schema}.items (id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY, label TEXT UNIQUE)"), QueryId::new()).await?;
    let dir = tempfile::tempdir().unwrap();
    let interceptor = InterceptorPipeline::new(dir.path().join("audit"));
    let insert = |label: &str| SandboxChangeDto {
        change_type: SandboxChangeType::Insert,
        namespace: namespace.clone(),
        table_name: "items".into(),
        primary_key: None,
        old_values: None,
        new_values: Some(
            RowData::new()
                .with_column("label", Value::Text(label.into()))
                .columns,
        ),
    };
    let result = apply_batch_with_capture(
        &manager,
        &interceptor,
        &QueryCache::new(),
        session,
        &[insert("first"), insert("second")],
        true,
        false,
        &[0, 1],
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.captures.len(), 2);
    for (index, capture) in &result.captures {
        assert_eq!(
            serde_json::to_value(&capture.primary_key.columns["id"]).unwrap(),
            *index + 1
        );
        assert!(capture.after.is_some());
    }
    assert!(!serde_json::to_string(&result).unwrap().contains("captures"));
    let result = apply_batch_with_capture(
        &manager,
        &interceptor,
        &QueryCache::new(),
        session,
        &[insert("rollback"), insert("first")],
        true,
        false,
        &[0, 1],
    )
    .await;
    assert!(!result.success);
    assert!(result.captures.is_empty());
    assert_eq!(result.applied_count, 0);
    let rows = driver
        .execute(
            session,
            &format!("SELECT COUNT(*) FROM {schema}.items"),
            QueryId::new(),
        )
        .await?;
    assert_count(&rows, 2);
    driver
        .execute(
            session,
            &format!("DROP SCHEMA {schema} CASCADE"),
            QueryId::new(),
        )
        .await?;
    manager.disconnect(session).await
}

// The fixture user needs CREATE ROLE, as supplied by the disposable test stack.
#[tokio::test]
async fn postgres_optional_returning_preserves_insert_only_and_rls_permissions() -> EngineResult<()>
{
    let Some((driver, session, config)) = connect_or_skip(
        connect_postgres().await,
        Service::Postgres,
        "postgres_optional_returning_preserves_insert_only_and_rls_permissions",
    )?
    else {
        return Ok(());
    };
    let schema = unique_name("capture_permissions");
    let role = unique_name("capture_writer");
    let namespace = Namespace {
        database: config.database.unwrap(),
        schema: Some(schema.clone()),
    };
    for sql in [
        format!("CREATE SCHEMA {schema}"),
        format!("CREATE ROLE {role}"),
        format!("GRANT USAGE ON SCHEMA {schema} TO {role}"),
        format!(
            "CREATE TABLE {schema}.items (id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY, label TEXT)"
        ),
        format!("GRANT INSERT ON {schema}.items TO {role}"),
    ] {
        driver.execute(session, &sql, QueryId::new()).await?;
    }
    for permissions in 0..3 {
        if permissions == 1 {
            driver
                .execute(
                    session,
                    &format!("GRANT SELECT (id) ON {schema}.items TO {role}"),
                    QueryId::new(),
                )
                .await?;
        }
        if permissions == 2 {
            for sql in [
                format!("GRANT SELECT ON {schema}.items TO {role}"),
                format!("ALTER TABLE {schema}.items ENABLE ROW LEVEL SECURITY"),
                format!(
                    "CREATE POLICY writer ON {schema}.items FOR INSERT TO {role} WITH CHECK (true)"
                ),
            ] {
                driver.execute(session, &sql, QueryId::new()).await?;
            }
        }
        driver.begin_transaction(session).await?;
        driver
            .execute(session, &format!("SET LOCAL ROLE {role}"), QueryId::new())
            .await?;
        let data = RowData::new().with_column("label", Value::Text("allowed".into()));
        let mut prepared = prepare_capture(
            driver.as_ref(),
            session,
            &namespace,
            "items",
            SandboxChangeType::Insert,
            &data,
        )
        .await;
        let outcome = driver
            .insert_row_returning(
                session,
                &namespace,
                "items",
                &data,
                prepared.returning_columns(),
            )
            .await?;
        assert_eq!(outcome.result.affected_rows, Some(1));
        assert!(outcome.returned_values.is_none());
        prepared.use_inserted_values(outcome.returned_values);
        let capture = finish_capture(
            driver.as_ref(),
            session,
            &namespace,
            "items",
            SandboxChangeType::Insert,
            &data,
            prepared,
            &outcome.result,
        )
        .await
        .unwrap();
        assert!(capture.primary_key.columns.is_empty());
        assert!(capture.after.is_none());
        driver.commit(session).await?;
    }
    let rows = driver
        .execute(
            session,
            &format!("SELECT COUNT(*) FROM {schema}.items"),
            QueryId::new(),
        )
        .await?;
    assert_count(&rows, 3);
    driver
        .execute(
            session,
            &format!("DROP SCHEMA {schema} CASCADE"),
            QueryId::new(),
        )
        .await?;
    driver
        .execute(session, &format!("DROP ROLE {role}"), QueryId::new())
        .await?;
    driver.disconnect(session).await
}

#[tokio::test]
async fn postgres_capture_read_failure_preserves_explicit_insert() -> EngineResult<()> {
    let Some((driver, session, config)) = connect_or_skip(
        connect_postgres().await,
        Service::Postgres,
        "postgres_capture_read_failure_preserves_explicit_insert",
    )?
    else {
        return Ok(());
    };
    let schema = unique_name("capture_read_failure");
    let role = unique_name("capture_write_only");
    let namespace = Namespace {
        database: config.database.unwrap(),
        schema: Some(schema.clone()),
    };
    for sql in [
        format!("CREATE SCHEMA {schema}"),
        format!("CREATE ROLE {role}"),
        format!("CREATE TABLE {schema}.items (id BIGINT PRIMARY KEY, label TEXT)"),
        format!("GRANT USAGE ON SCHEMA {schema} TO {role}"),
        format!("GRANT INSERT, UPDATE, DELETE ON {schema}.items TO {role}"),
        format!("GRANT SELECT (id) ON {schema}.items TO {role}"),
    ] {
        driver.execute(session, &sql, QueryId::new()).await?;
    }
    let key = RowData::new().with_column("id", Value::Int(1));
    for operation in [
        SandboxChangeType::Insert,
        SandboxChangeType::Update,
        SandboxChangeType::Delete,
    ] {
        driver.begin_transaction(session).await?;
        driver
            .execute(session, &format!("SET LOCAL ROLE {role}"), QueryId::new())
            .await?;
        let data = key
            .clone()
            .with_column("label", Value::Text("allowed".into()));
        let mut prepared = prepare_capture(
            driver.as_ref(),
            session,
            &namespace,
            "items",
            operation.clone(),
            &key,
        )
        .await;
        let result = match operation {
            SandboxChangeType::Insert => {
                let outcome = driver
                    .insert_row_returning(
                        session,
                        &namespace,
                        "items",
                        &data,
                        prepared.returning_columns(),
                    )
                    .await?;
                prepared.use_inserted_values(outcome.returned_values);
                outcome.result
            }
            SandboxChangeType::Update => {
                driver
                    .update_row(session, &namespace, "items", &key, &data)
                    .await?
            }
            SandboxChangeType::Delete => {
                driver
                    .delete_row(session, &namespace, "items", &key)
                    .await?
            }
        };
        let capture = finish_capture(
            driver.as_ref(),
            session,
            &namespace,
            "items",
            operation.clone(),
            &data,
            prepared,
            &result,
        )
        .await
        .unwrap();
        assert!(capture.primary_key.columns.is_empty());
        assert!(capture.before.is_none());
        assert!(capture.after.is_none());
        driver.commit(session).await?;
        let rows = driver
            .execute(
                session,
                &format!("SELECT COUNT(*) FROM {schema}.items"),
                QueryId::new(),
            )
            .await?;
        assert_count(
            &rows,
            if matches!(operation, SandboxChangeType::Delete) {
                0
            } else {
                1
            },
        );
    }
    driver
        .execute(
            session,
            &format!("DROP SCHEMA {schema} CASCADE"),
            QueryId::new(),
        )
        .await?;
    driver
        .execute(session, &format!("DROP ROLE {role}"), QueryId::new())
        .await?;
    driver.disconnect(session).await
}

#[tokio::test]
async fn postgres_commit_rejects_an_aborted_transaction() -> EngineResult<()> {
    let Some((driver, session, _)) = connect_or_skip(
        connect_postgres().await,
        Service::Postgres,
        "postgres_commit_rejects_an_aborted_transaction",
    )?
    else {
        return Ok(());
    };
    driver.begin_transaction(session).await?;
    assert!(
        driver
            .execute(session, "SELECT 1 / 0", QueryId::new())
            .await
            .is_err()
    );
    assert!(
        driver.commit(session).await.is_err(),
        "an aborted transaction must never be reported as committed"
    );
    driver.begin_transaction(session).await?;
    driver.execute(session, "SELECT 1", QueryId::new()).await?;
    driver.commit(session).await?;
    driver.disconnect(session).await
}

fn capture_lookup(column: &str) -> TableQueryOptions {
    TableQueryOptions {
        page: Some(1),
        page_size: Some(2),
        count_mode: Some(CountMode::None),
        filters: Some(vec![qore_core::ColumnFilter {
            column: column.into(),
            operator: qore_core::FilterOperator::Eq,
            value: Value::Int(1),
            options: Default::default(),
        }]),
        ..Default::default()
    }
}

#[tokio::test]
async fn postgres_capture_sql_error_restores_the_transaction() -> EngineResult<()> {
    let Some((driver, session, config)) = connect_or_skip(
        connect_postgres().await,
        Service::Postgres,
        "postgres_capture_sql_error_restores_the_transaction",
    )?
    else {
        return Ok(());
    };
    let table = unique_name("capture_error");
    let namespace = Namespace {
        database: config.database.unwrap(),
        schema: Some("public".into()),
    };
    driver
        .execute(
            session,
            &format!("CREATE TABLE {table} (id BIGINT PRIMARY KEY)"),
            QueryId::new(),
        )
        .await?;
    driver.begin_transaction(session).await?;
    driver
        .execute(
            session,
            &format!("INSERT INTO {table} VALUES (1)"),
            QueryId::new(),
        )
        .await?;
    assert!(
        driver
            .query_table_for_capture(
                session,
                &namespace,
                &table,
                capture_lookup("missing_column")
            )
            .await
            .is_err()
    );
    let page = driver
        .query_table_for_capture(session, &namespace, &table, capture_lookup("id"))
        .await?;
    assert_eq!(page.result.rows.len(), 1);
    assert_eq!(
        serde_json::to_value(&page.result.rows[0].values[0]).unwrap(),
        1
    );
    driver.commit(session).await?;
    let count = driver
        .execute(
            session,
            &format!("SELECT COUNT(*) FROM {table}"),
            QueryId::new(),
        )
        .await?;
    assert_count(&count, 1);
    driver
        .execute(session, &format!("DROP TABLE {table}"), QueryId::new())
        .await?;
    driver.disconnect(session).await
}

#[tokio::test]
async fn postgres_cancelled_capture_recovers_before_the_next_write() -> EngineResult<()> {
    let Some((driver, session, config)) = connect_or_skip(
        connect_postgres().await,
        Service::Postgres,
        "postgres_cancelled_capture_recovers_before_the_next_write",
    )?
    else {
        return Ok(());
    };
    let schema = unique_name("capture_cancel");
    let namespace = Namespace {
        database: config.database.clone().unwrap(),
        schema: Some(schema.clone()),
    };
    for sql in [
        format!("CREATE SCHEMA {schema}"),
        format!("CREATE TABLE {schema}.items (id BIGINT PRIMARY KEY)"),
        format!(
            "CREATE VIEW {schema}.slow AS SELECT id, pg_sleep(10) IS NULL AS waited FROM {schema}.items"
        ),
    ] {
        driver.execute(session, &sql, QueryId::new()).await?;
    }
    driver.begin_transaction(session).await?;
    driver
        .execute(
            session,
            "SET LOCAL statement_timeout = '5s'",
            QueryId::new(),
        )
        .await?;
    driver
        .execute(
            session,
            &format!("INSERT INTO {schema}.items VALUES (1)"),
            QueryId::new(),
        )
        .await?;
    let pid = driver
        .execute(session, "SELECT pg_backend_pid()::bigint", QueryId::new())
        .await?;
    let Value::Int(pid) = pid.rows[0].values[0] else {
        panic!("backend PID missing")
    };
    let admin = driver.connect(&config).await?;
    let read = tokio::spawn({
        let driver = driver.clone();
        let namespace = namespace.clone();
        async move {
            driver
                .query_table_for_capture(session, &namespace, "slow", capture_lookup("id"))
                .await
        }
    });
    timeout(Duration::from_secs(3), async {
        loop {
            let status = driver.execute(admin, &format!("SELECT COUNT(*) FROM pg_stat_activity WHERE pid = {pid} AND wait_event = 'PgSleep'"), QueryId::new()).await.unwrap();
            if matches!(status.rows[0].values[0], Value::Int(1)) { break; }
            sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("capture query never reached pg_sleep");
    read.abort();
    assert!(read.await.unwrap_err().is_cancelled());
    // This command must wait for the detached recovery task. It must neither
    // see an aborted transaction nor be rolled back with the optional read.
    timeout(
        Duration::from_secs(6),
        driver.execute(
            session,
            &format!("INSERT INTO {schema}.items VALUES (2)"),
            QueryId::new(),
        ),
    )
    .await
    .unwrap()?;
    let setting = driver
        .execute(session, "SHOW statement_timeout", QueryId::new())
        .await?;
    assert_eq!(
        serde_json::to_value(&setting.rows[0].values[0]).unwrap(),
        "5s"
    );
    driver.commit(session).await?;
    let count = driver
        .execute(
            session,
            &format!("SELECT COUNT(*) FROM {schema}.items"),
            QueryId::new(),
        )
        .await?;
    assert_count(&count, 2);
    driver
        .execute(
            session,
            &format!("DROP SCHEMA {schema} CASCADE"),
            QueryId::new(),
        )
        .await?;
    driver.disconnect(admin).await?;
    driver.disconnect(session).await
}

#[tokio::test]
async fn postgres_capture_honors_stricter_timeout_and_caller_savepoint() -> EngineResult<()> {
    let Some((driver, session, config)) = connect_or_skip(
        connect_postgres().await,
        Service::Postgres,
        "postgres_capture_honors_stricter_timeout_and_caller_savepoint",
    )?
    else {
        return Ok(());
    };
    let schema = unique_name("capture_deadline");
    let namespace = Namespace {
        database: config.database.unwrap(),
        schema: Some(schema.clone()),
    };
    for sql in [
        format!("CREATE SCHEMA {schema}"),
        format!("CREATE TABLE {schema}.items (id BIGINT PRIMARY KEY)"),
        format!(
            "CREATE VIEW {schema}.slow AS SELECT id, pg_sleep(1) IS NULL AS waited FROM {schema}.items"
        ),
    ] {
        driver.execute(session, &sql, QueryId::new()).await?;
    }
    driver.begin_transaction(session).await?;
    driver
        .execute(
            session,
            &format!("INSERT INTO {schema}.items VALUES (1)"),
            QueryId::new(),
        )
        .await?;
    driver
        .execute(session, "SAVEPOINT caller_savepoint", QueryId::new())
        .await?;
    driver
        .execute(
            session,
            "SET LOCAL statement_timeout = '100ms'",
            QueryId::new(),
        )
        .await?;
    assert!(
        driver
            .query_table_for_capture(session, &namespace, "slow", capture_lookup("id"))
            .await
            .is_err(),
        "must honor 100ms rather than raising it to the capture timeout"
    );
    let setting = driver
        .execute(session, "SHOW statement_timeout", QueryId::new())
        .await?;
    assert_eq!(
        serde_json::to_value(&setting.rows[0].values[0]).unwrap(),
        "100ms"
    );
    driver
        .execute(
            session,
            &format!("INSERT INTO {schema}.items VALUES (2)"),
            QueryId::new(),
        )
        .await?;
    driver
        .execute(
            session,
            "ROLLBACK TO SAVEPOINT caller_savepoint",
            QueryId::new(),
        )
        .await?;
    driver.commit(session).await?;
    let count = driver
        .execute(
            session,
            &format!("SELECT COUNT(*) FROM {schema}.items"),
            QueryId::new(),
        )
        .await?;
    assert_count(&count, 1);
    driver
        .execute(
            session,
            &format!("DROP SCHEMA {schema} CASCADE"),
            QueryId::new(),
        )
        .await?;
    driver.disconnect(session).await
}

#[tokio::test]
async fn postgres_batch_with_unreadable_images_keeps_confirmed_writes() -> EngineResult<()> {
    let Some((driver, admin, config)) = connect_or_skip(
        connect_postgres().await,
        Service::Postgres,
        "postgres_batch_with_unreadable_images_keeps_confirmed_writes",
    )?
    else {
        return Ok(());
    };
    let schema = unique_name("capture_batch_role");
    let role = unique_name("capture_batch_writer");
    let namespace = Namespace {
        database: config.database.clone().unwrap(),
        schema: Some(schema.clone()),
    };
    for sql in [
        format!("CREATE SCHEMA {schema}"),
        format!("CREATE ROLE {role} LOGIN PASSWORD 'qoredb_capture_fixture'"),
        format!("CREATE TABLE {schema}.items (id BIGINT PRIMARY KEY, label TEXT)"),
        format!("GRANT USAGE ON SCHEMA {schema} TO {role}"),
        format!("GRANT INSERT ON {schema}.items TO {role}"),
        format!("GRANT SELECT (id) ON {schema}.items TO {role}"),
    ] {
        driver.execute(admin, &sql, QueryId::new()).await?;
    }
    let mut registry = DriverRegistry::new();
    registry.register(driver.clone());
    let manager = SessionManager::new(Arc::new(registry));
    let writer = manager
        .connect(ConnectionConfig {
            username: role.clone(),
            password: "qoredb_capture_fixture".into(),
            ..config
        })
        .await?;
    let dir = tempfile::tempdir().unwrap();
    let interceptor = InterceptorPipeline::new(dir.path().join("audit"));
    let change = |id| SandboxChangeDto {
        change_type: SandboxChangeType::Insert,
        namespace: namespace.clone(),
        table_name: "items".into(),
        primary_key: None,
        old_values: None,
        new_values: Some(
            RowData::new()
                .with_column("id", Value::Int(id))
                .with_column("label", Value::Text("allowed".into()))
                .columns,
        ),
    };
    let result = apply_batch_with_capture(
        &manager,
        &interceptor,
        &QueryCache::new(),
        writer,
        &[change(1), change(2)],
        true,
        false,
        &[0, 1],
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.applied_count, 2);
    assert_eq!(result.captures.len(), 2);
    assert!(
        result
            .captures
            .iter()
            .all(|(_, capture)| capture.primary_key.columns.is_empty() && capture.after.is_none())
    );
    let count = driver
        .execute(
            admin,
            &format!("SELECT COUNT(*) FROM {schema}.items"),
            QueryId::new(),
        )
        .await?;
    assert_count(&count, 2);
    let result = apply_batch_with_capture(
        &manager,
        &interceptor,
        &QueryCache::new(),
        writer,
        &[change(3), change(1)],
        true,
        false,
        &[0, 1],
    )
    .await;
    assert!(!result.success);
    assert_eq!(result.applied_count, 0);
    assert!(result.captures.is_empty());
    let count = driver
        .execute(
            admin,
            &format!("SELECT COUNT(*) FROM {schema}.items"),
            QueryId::new(),
        )
        .await?;
    assert_count(&count, 2);
    manager.disconnect(writer).await?;
    driver
        .execute(
            admin,
            &format!("DROP SCHEMA {schema} CASCADE"),
            QueryId::new(),
        )
        .await?;
    driver
        .execute(admin, &format!("DROP ROLE {role}"), QueryId::new())
        .await?;
    driver.disconnect(admin).await
}

#[tokio::test]
async fn postgres_lost_capture_connection_cannot_fall_back_to_autocommit() -> EngineResult<()> {
    let Some((driver, session, config)) = connect_or_skip(
        connect_postgres().await,
        Service::Postgres,
        "postgres_lost_capture_connection_cannot_fall_back_to_autocommit",
    )?
    else {
        return Ok(());
    };
    let schema = unique_name("capture_disconnect");
    let namespace = Namespace {
        database: config.database.clone().unwrap(),
        schema: Some(schema.clone()),
    };
    for sql in [
        format!("CREATE SCHEMA {schema}"),
        format!("CREATE TABLE {schema}.items (id BIGINT PRIMARY KEY)"),
        format!(
            "CREATE VIEW {schema}.slow AS SELECT id, pg_sleep(10) IS NULL AS waited FROM {schema}.items"
        ),
    ] {
        driver.execute(session, &sql, QueryId::new()).await?;
    }
    driver.begin_transaction(session).await?;
    driver
        .execute(
            session,
            &format!("INSERT INTO {schema}.items VALUES (1)"),
            QueryId::new(),
        )
        .await?;
    let pid = driver
        .execute(session, "SELECT pg_backend_pid()::bigint", QueryId::new())
        .await?;
    let Value::Int(pid) = pid.rows[0].values[0] else {
        panic!("backend PID missing")
    };
    let admin = driver.connect(&config).await?;
    let read = tokio::spawn({
        let driver = driver.clone();
        let namespace = namespace.clone();
        async move {
            driver
                .query_table_for_capture(session, &namespace, "slow", capture_lookup("id"))
                .await
        }
    });
    timeout(Duration::from_secs(3), async {
        loop {
            let status = driver.execute(admin, &format!("SELECT COUNT(*) FROM pg_stat_activity WHERE pid = {pid} AND wait_event = 'PgSleep'"), QueryId::new()).await.unwrap();
            if matches!(status.rows[0].values[0], Value::Int(1)) { break; }
            sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("capture query never reached pg_sleep");
    driver
        .execute(
            admin,
            &format!("SELECT pg_terminate_backend({pid})"),
            QueryId::new(),
        )
        .await?;
    assert!(
        timeout(Duration::from_secs(6), read)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    let next = driver
        .insert_row(
            session,
            &namespace,
            "items",
            &RowData::new().with_column("id", Value::Int(2)),
        )
        .await;
    assert!(
        next.is_err(),
        "a lost transaction must not turn the next batch write into autocommit"
    );
    assert!(driver.commit(session).await.is_err());
    let count = driver
        .execute(
            admin,
            &format!("SELECT COUNT(*) FROM {schema}.items"),
            QueryId::new(),
        )
        .await?;
    assert_count(&count, 0);
    driver.begin_transaction(session).await?;
    driver
        .insert_row(
            session,
            &namespace,
            "items",
            &RowData::new().with_column("id", Value::Int(3)),
        )
        .await?;
    driver.commit(session).await?;
    let count = driver
        .execute(
            admin,
            &format!("SELECT COUNT(*) FROM {schema}.items"),
            QueryId::new(),
        )
        .await?;
    assert_count(&count, 1);
    driver
        .execute(
            admin,
            &format!("DROP SCHEMA {schema} CASCADE"),
            QueryId::new(),
        )
        .await?;
    driver.disconnect(admin).await?;
    driver.disconnect(session).await
}
