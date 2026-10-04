// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::interceptor::InterceptorPipeline;
use async_trait::async_trait;
use qore_core::{
    CollectionList, CollectionListOptions, ConnectionConfig, DriverRegistry, EngineError,
    EngineResult, Namespace, QueryId, QueryResult, TableSchema, Value,
};
use qore_drivers::{drivers::sqlite::SqliteDriver, session_manager::SessionManager};

enum CommitFailure {
    None,
    BeforeCommit,
    AfterCommit,
}

struct TestDriver {
    sqlite: SqliteDriver,
    commit_failure: CommitFailure,
    fail_rollback: bool,
    transactions: bool,
}

#[async_trait]
impl DataEngine for TestDriver {
    fn driver_id(&self) -> &'static str {
        "sqlite"
    }
    fn driver_name(&self) -> &'static str {
        "SQLite fault fixture"
    }
    fn supports_mutations(&self) -> bool {
        true
    }
    fn supports_transactions(&self) -> bool {
        self.transactions
    }
    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        self.sqlite.test_connection(config).await
    }
    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        self.sqlite.connect(config).await
    }
    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        self.sqlite.disconnect(session).await
    }
    async fn ping(&self, session: SessionId) -> EngineResult<()> {
        self.sqlite.ping(session).await
    }
    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        self.sqlite.list_namespaces(session).await
    }
    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        self.sqlite
            .list_collections(session, namespace, options)
            .await
    }
    async fn create_database(
        &self,
        session: SessionId,
        name: &str,
        options: Option<Value>,
    ) -> EngineResult<()> {
        self.sqlite.create_database(session, name, options).await
    }
    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        self.sqlite.drop_database(session, name).await
    }
    async fn execute(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        self.sqlite.execute(session, query, query_id).await
    }
    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        self.sqlite.describe_table(session, namespace, table).await
    }
    async fn preview_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        self.sqlite
            .preview_table(session, namespace, table, limit)
            .await
    }
    async fn insert_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        self.sqlite
            .insert_row(session, namespace, table, data)
            .await
    }
    async fn update_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        pk: &RowData,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        self.sqlite
            .update_row(session, namespace, table, pk, data)
            .await
    }
    async fn delete_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        pk: &RowData,
    ) -> EngineResult<QueryResult> {
        self.sqlite.delete_row(session, namespace, table, pk).await
    }
    async fn begin_transaction(&self, session: SessionId) -> EngineResult<()> {
        self.sqlite.begin_transaction(session).await
    }
    async fn commit(&self, session: SessionId) -> EngineResult<()> {
        match self.commit_failure {
            CommitFailure::None => self.sqlite.commit(session).await,
            CommitFailure::BeforeCommit => Err(EngineError::connection_failed(
                "commit failed password=synthetic-secret",
            )),
            CommitFailure::AfterCommit => {
                self.sqlite.commit(session).await?;
                Err(EngineError::connection_failed(
                    "commit response lost password=synthetic-secret",
                ))
            }
        }
    }

    async fn rollback(&self, session: SessionId) -> EngineResult<()> {
        if self.fail_rollback {
            Err(EngineError::connection_failed(
                "rollback failed password=synthetic-secret",
            ))
        } else {
            self.sqlite.rollback(session).await
        }
    }
}

struct Fixture {
    driver: Arc<dyn DataEngine>,
    manager: SessionManager,
    interceptor: InterceptorPipeline,
    session: SessionId,
    namespace: Namespace,
    config: ConnectionConfig,
    _dir: tempfile::TempDir,
}

async fn fixture(fail_commit: bool, fail_rollback: bool, transactions: bool) -> Fixture {
    fixture_with_commit_failure(
        if fail_commit {
            CommitFailure::BeforeCommit
        } else {
            CommitFailure::None
        },
        fail_rollback,
        transactions,
    )
    .await
}

async fn fixture_with_commit_failure(
    commit_failure: CommitFailure,
    fail_rollback: bool,
    transactions: bool,
) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let driver: Arc<dyn DataEngine> = Arc::new(TestDriver {
        sqlite: SqliteDriver::new(),
        commit_failure,
        fail_rollback,
        transactions,
    });
    let mut registry = DriverRegistry::new();
    registry.register(Arc::clone(&driver));
    let manager = SessionManager::new(Arc::new(registry));
    let config: ConnectionConfig = serde_json::from_value(serde_json::json!({
        "driver": "sqlite", "host": dir.path().join("batch.db").to_str().unwrap(),
        "port": 0, "username": "", "password": "", "ssl": false,
        "environment": "development", "read_only": false
    }))
    .unwrap();
    let session = manager.connect(config.clone()).await.unwrap();
    driver
        .execute(
            session,
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)",
            QueryId::new(),
        )
        .await
        .unwrap();
    let namespace = driver.list_namespaces(session).await.unwrap().remove(0);
    Fixture {
        driver,
        manager,
        interceptor: InterceptorPipeline::new(dir.path().join("audit")),
        session,
        namespace,
        config,
        _dir: dir,
    }
}

fn insert(f: &Fixture, id: i64) -> SandboxChangeDto {
    SandboxChangeDto {
        change_type: SandboxChangeType::Insert,
        namespace: f.namespace.clone(),
        table_name: "items".into(),
        primary_key: None,
        old_values: None,
        new_values: Some(
            RowData::new()
                .with_column("id", Value::Int(id))
                .with_column("name", Value::Text("fixture".into()))
                .columns,
        ),
    }
}

async fn count(f: &Fixture) -> i64 {
    let result = f
        .driver
        .execute(f.session, "SELECT count(*) FROM items", QueryId::new())
        .await
        .unwrap();
    match result.rows[0].values[0] {
        Value::Int(n) => n,
        ref other => panic!("unexpected count: {other:?}"),
    }
}

#[tokio::test]
async fn rolled_back_batch_has_no_confirmed_changes() {
    let f = fixture(false, false, true).await;
    let result = execute_batch(&f.driver, f.session, &[insert(&f, 1), insert(&f, 1)], true).await;
    assert!(!result.success);
    assert_eq!(count(&f).await, 0);
    assert_eq!(result.applied_count, 0);
    assert!(result.applied_indices.is_empty());
    assert!(!result.outcome_unknown);
}

#[tokio::test]
async fn failed_commit_is_unknown_and_never_confirmed() {
    let f = fixture(true, false, true).await;
    let result = execute_batch(&f.driver, f.session, &[insert(&f, 1)], true).await;
    assert!(!result.success);
    assert!(result.outcome_unknown);
    assert!(result.applied_indices.is_empty());
    assert_eq!(result.applied_count, 0);
    assert!(!result.error.unwrap().contains("synthetic-secret"));
    assert_eq!(count(&f).await, 0);
}

#[tokio::test]
async fn required_transaction_never_silently_falls_back_to_partial_writes() {
    let f = fixture(false, false, false).await;
    let result = execute_batch(&f.driver, f.session, &[insert(&f, 1)], true).await;
    assert!(!result.success);
    assert_eq!(count(&f).await, 0);
    assert!(result.applied_indices.is_empty());
}

async fn guarded(
    f: &Fixture,
    changes: &[SandboxChangeDto],
    acknowledged: bool,
) -> ApplyBatchResult {
    apply_batch(
        &f.manager,
        &f.interceptor,
        &crate::cache::QueryCache::new(),
        f.session,
        changes,
        true,
        acknowledged,
    )
    .await
}

fn delete(f: &Fixture, id: i64) -> SandboxChangeDto {
    SandboxChangeDto {
        change_type: SandboxChangeType::Delete,
        namespace: f.namespace.clone(),
        table_name: "items".into(),
        primary_key: Some(RowData::new().with_column("id", Value::Int(id))),
        old_values: None,
        new_values: None,
    }
}

#[tokio::test]
async fn successful_commit_confirms_every_index_and_preserves_large_integers() {
    let f = fixture(false, false, true).await;
    let id = 9_007_199_254_740_993;
    let result = guarded(&f, &[insert(&f, id), insert(&f, 2)], false).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.applied_indices, vec![0, 1]);
    assert_eq!(result.applied_count, 2);
    let rows = f
        .driver
        .execute(
            f.session,
            "SELECT id FROM items ORDER BY id DESC",
            QueryId::new(),
        )
        .await
        .unwrap();
    assert!(matches!(rows.rows[0].values[0], Value::Int(n) if n == id));
}

#[tokio::test]
async fn failed_rollback_is_unknown_and_redacts_secrets() {
    let f = fixture(false, true, true).await;
    let result = guarded(&f, &[insert(&f, 1), insert(&f, 1)], false).await;
    assert!(!result.success);
    assert!(result.outcome_unknown);
    assert_eq!(result.applied_count, 0);
    assert!(result.applied_indices.is_empty());
    assert!(!result.error.unwrap().contains("synthetic-secret"));
    f.driver.disconnect(f.session).await.unwrap();
}

#[tokio::test]
async fn begin_failure_does_not_rollback_an_existing_transaction() {
    let f = fixture(false, false, true).await;
    f.driver.begin_transaction(f.session).await.unwrap();
    apply_single_change(&f.driver, f.session, &insert(&f, 1))
        .await
        .unwrap();
    let result = guarded(&f, &[insert(&f, 2)], false).await;
    assert!(!result.success);
    assert_eq!(result.applied_count, 0);
    f.driver.commit(f.session).await.unwrap();
    assert_eq!(count(&f).await, 1);
}

#[tokio::test]
async fn explicit_nontransactional_batch_reports_only_successes_and_stops_on_failure() {
    let f = fixture(false, false, false).await;
    let result = execute_batch(
        &f.driver,
        f.session,
        &[insert(&f, 1), insert(&f, 1), insert(&f, 3)],
        false,
    )
    .await;
    assert!(!result.success);
    assert_eq!(result.applied_indices, vec![0]);
    assert_eq!(result.applied_count, 1);
    assert_eq!(result.failed_changes[0].index, 1);
    assert_eq!(count(&f).await, 1);
}

#[tokio::test]
async fn empty_key_is_rejected_before_any_batch_write() {
    let f = fixture(false, false, true).await;
    let mut change = delete(&f, 1);
    change.primary_key = Some(RowData::new());
    let result = guarded(&f, &[insert(&f, 1), change], false).await;
    assert!(!result.success);
    assert_eq!(result.failed_changes[0].index, 1);
    assert_eq!(count(&f).await, 0);
}

#[tokio::test]
async fn masking_checks_the_whole_batch_before_writing() {
    use qore_core::masking::{ConnectionMasking, MaskMode, MaskingRule};
    let f = fixture(false, false, true).await;
    f.manager
        .set_saved_connection_identity(f.session, "fixture-connection".into(), "Fixture".into())
        .await;
    f.manager
        .set_masking(
            f.session,
            &ConnectionMasking {
                rules: vec![MaskingRule {
                    table: "items".into(),
                    column: "name".into(),
                    mode: MaskMode::Hidden,
                }],
                mask_detected_columns: false,
            },
        )
        .await;
    let mut safe = insert(&f, 1);
    safe.new_values.as_mut().unwrap().remove("name");
    let result = guarded(&f, &[safe, insert(&f, 2)], false).await;
    assert_eq!(result.error.as_deref(), Some("MASKED_FIELD_UPDATE"));
    assert_eq!(count(&f).await, 0);
    let mut change = delete(&f, 1);
    change.primary_key = Some(RowData::new().with_column("name", Value::Text("masked".into())));
    assert_eq!(
        guarded(&f, &[change], false).await.error.as_deref(),
        Some("MASKED_FIELD_UPDATE")
    );
}

#[tokio::test]
async fn production_delete_requires_backend_acknowledgement() {
    let mut f = fixture(false, false, true).await;
    f.config.environment = "production".into();
    f.session = f.manager.connect(f.config.clone()).await.unwrap();
    let changes = [insert(&f, 1), delete(&f, 1)];
    let result = guarded(&f, &changes, false).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("confirmation required"));
    assert_eq!(count(&f).await, 0);
    assert!(guarded(&f, &changes, true).await.success);
    assert_eq!(count(&f).await, 0);
}

#[tokio::test]
async fn read_only_connection_refuses_batch() {
    let mut f = fixture(false, false, true).await;
    f.config.read_only = true;
    f.session = f.manager.connect(f.config.clone()).await.unwrap();
    let result = guarded(&f, &[insert(&f, 1)], true).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("read-only"));
    assert_eq!(count(&f).await, 0);
}

#[tokio::test]
async fn custom_safety_block_cannot_be_acknowledged_away() {
    use crate::interceptor::{Environment, QueryOperationType, SafetyAction, SafetyRule};
    let f = fixture(false, false, true).await;
    f.interceptor
        .add_safety_rule(SafetyRule {
            id: "fixture-block".into(),
            name: "fixture block".into(),
            description: String::new(),
            enabled: true,
            environments: vec![Environment::Development],
            operations: vec![QueryOperationType::Delete],
            action: SafetyAction::Block,
            pattern: None,
            builtin: false,
        })
        .unwrap();
    let result = guarded(&f, &[insert(&f, 1), delete(&f, 1)], true).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("safety rule"));
    assert_eq!(count(&f).await, 0);
}

#[tokio::test]
async fn batch_invalidates_its_connection_cache_and_audits_rolled_back_writes() {
    let f = fixture(false, false, true).await;
    let cache = crate::cache::QueryCache::new();
    cache.set_config(crate::cache::CacheConfig::default());
    let key = f.manager.connection_key(f.session).await.unwrap();
    cache.put("fixture-query".into(), key, "stale".into());
    cache.put(
        "other-query".into(),
        "other-connection".into(),
        "other".into(),
    );
    assert!(cache.get("fixture-query").is_some());
    let result = apply_batch(
        &f.manager,
        &f.interceptor,
        &cache,
        f.session,
        &[insert(&f, 1), insert(&f, 1)],
        true,
        false,
    )
    .await;
    assert!(!result.success);
    assert!(cache.get("fixture-query").is_none());
    assert!(cache.get("other-query").is_some());
    let entries = f
        .interceptor
        .get_audit_entries(10, 0, None, None, None, None, None, None);
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .all(|entry| !entry.success && !entry.query.contains("fixture"))
    );
}

#[tokio::test]
async fn lost_commit_response_never_claims_rollback_or_confirmed_history() {
    let f = fixture_with_commit_failure(CommitFailure::AfterCommit, false, true).await;
    let result = guarded(&f, &[insert(&f, 1)], false).await;
    assert_eq!(count(&f).await, 1);
    assert!(!result.success);
    assert!(result.outcome_unknown);
    assert_eq!(result.applied_count, 0);
    assert!(result.applied_indices.is_empty());
    assert!(!result.error.unwrap().contains("synthetic-secret"));
}

#[tokio::test]
async fn missing_target_rolls_back_previous_writes() {
    let f = fixture(false, false, true).await;
    let result = guarded(&f, &[insert(&f, 1), delete(&f, 404)], false).await;
    assert!(!result.success);
    assert!(!result.outcome_unknown);
    assert_eq!(result.applied_count, 0);
    assert_eq!(count(&f).await, 0);
}
