// SPDX-License-Identifier: Apache-2.0

//! Test doubles for [`DataEngine`]. Shared across command and service test
//! modules so that runners taking a plain `Arc<dyn DataEngine>` can be driven
//! without a database.

use async_trait::async_trait;
use qore_core::error::{EngineError, EngineResult};
use qore_core::traits::DataEngine;
use qore_core::types::{
    CollectionList, CollectionListOptions, ConnectionConfig, CreationOptions, Namespace, QueryId,
    QueryResult, SessionId, TableSchema, Value,
};
use std::collections::HashMap;
use std::sync::Mutex;

/// One driver interaction, in call order. Migration-runner invariants are
/// ordering invariants (claim before script, rollback after failure), so tests
/// need the sequence, not just the set of queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverCall {
    Execute(String),
    Begin,
    Commit,
    Rollback,
}

/// Mock driver that returns pre-canned results keyed by the SQL fragment
/// looked up in `responses` (substring match).
pub struct MockDriver {
    driver_id: &'static str,
    responses: Mutex<HashMap<String, EngineResult<QueryResult>>>,
    calls: Mutex<Vec<String>>,
    namespaces: Mutex<Vec<Option<Namespace>>>,
    log: Mutex<Vec<DriverCall>>,
    default: Mutex<Option<QueryResult>>,
    /// (0-based execute index, message) to fail on.
    fail_nth: Mutex<Option<(usize, String)>>,
    execute_count: Mutex<usize>,
    supports_tx: bool,
}

impl MockDriver {
    pub fn new(driver_id: &'static str) -> Self {
        Self {
            driver_id,
            responses: Mutex::new(HashMap::new()),
            calls: Mutex::new(Vec::new()),
            namespaces: Mutex::new(Vec::new()),
            log: Mutex::new(Vec::new()),
            default: Mutex::new(None),
            fail_nth: Mutex::new(None),
            execute_count: Mutex::new(0),
            supports_tx: false,
        }
    }

    pub fn add(&self, needle: &str, result: QueryResult) {
        self.responses
            .lock()
            .unwrap()
            .insert(needle.to_string(), Ok(result));
    }

    pub fn add_err(&self, needle: &str, msg: &str) {
        self.responses
            .lock()
            .unwrap()
            .insert(needle.to_string(), Err(EngineError::internal(msg)));
    }

    /// Executed queries, in order. Excludes transaction control.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    /// Namespace supplied for each `execute_in_namespace` call.
    pub fn namespace_calls(&self) -> Vec<Option<Namespace>> {
        self.namespaces.lock().unwrap().clone()
    }

    /// Every interaction in order, transaction control included.
    pub fn call_log(&self) -> Vec<DriverCall> {
        self.log.lock().unwrap().clone()
    }

    /// Response for queries no `add` needle matches. Without it every test would
    /// have to register the runner's internal bookkeeping SQL by hand.
    pub fn set_default(&self, result: QueryResult) {
        *self.default.lock().unwrap() = Some(result);
    }

    /// Fails the n-th `execute` (0-based), regardless of registered responses.
    pub fn fail_nth_execute(&self, n: usize, msg: &str) {
        *self.fail_nth.lock().unwrap() = Some((n, msg.to_string()));
    }

    pub fn with_transactions(mut self, yes: bool) -> Self {
        self.supports_tx = yes;
        self
    }
}

pub fn empty_result() -> QueryResult {
    QueryResult {
        columns: Vec::new(),
        rows: Vec::new(),
        affected_rows: None,
        execution_time_ms: 0.0,
    }
}

pub fn affected_result(n: u64) -> QueryResult {
    QueryResult {
        affected_rows: Some(n),
        ..empty_result()
    }
}

#[async_trait]
impl DataEngine for MockDriver {
    fn driver_id(&self) -> &'static str {
        self.driver_id
    }
    fn driver_name(&self) -> &'static str {
        "Mock"
    }
    async fn test_connection(&self, _config: &ConnectionConfig) -> EngineResult<()> {
        Ok(())
    }
    async fn connect(&self, _config: &ConnectionConfig) -> EngineResult<SessionId> {
        Ok(SessionId::new())
    }
    async fn disconnect(&self, _session: SessionId) -> EngineResult<()> {
        Ok(())
    }
    async fn ping(&self, _session: SessionId) -> EngineResult<()> {
        Ok(())
    }
    async fn list_namespaces(&self, _session: SessionId) -> EngineResult<Vec<Namespace>> {
        Ok(Vec::new())
    }
    async fn list_collections(
        &self,
        _session: SessionId,
        _namespace: &Namespace,
        _options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        Ok(CollectionList {
            collections: Vec::new(),
            total_count: 0,
        })
    }
    async fn create_database(
        &self,
        _session: SessionId,
        _name: &str,
        _options: Option<Value>,
    ) -> EngineResult<()> {
        Ok(())
    }
    async fn drop_database(&self, _session: SessionId, _name: &str) -> EngineResult<()> {
        Ok(())
    }
    async fn execute(
        &self,
        _session: SessionId,
        query: &str,
        _query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        self.calls.lock().unwrap().push(query.to_string());
        self.log
            .lock()
            .unwrap()
            .push(DriverCall::Execute(query.to_string()));

        let n = {
            let mut c = self.execute_count.lock().unwrap();
            let n = *c;
            *c += 1;
            n
        };
        if let Some((target, msg)) = self.fail_nth.lock().unwrap().as_ref() {
            if *target == n {
                return Err(EngineError::internal(msg.clone()));
            }
        }

        let map = self.responses.lock().unwrap();
        for (needle, response) in map.iter() {
            if query.contains(needle) {
                return match response {
                    Ok(r) => Ok(r.clone()),
                    Err(e) => Err(EngineError::internal(e.to_string())),
                };
            }
        }
        if let Some(d) = self.default.lock().unwrap().as_ref() {
            return Ok(d.clone());
        }
        Err(EngineError::internal(format!("no mock for: {query}")))
    }
    async fn execute_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        self.namespaces.lock().unwrap().push(namespace);
        self.execute(session, query, query_id).await
    }
    async fn begin_transaction(&self, _session: SessionId) -> EngineResult<()> {
        self.log.lock().unwrap().push(DriverCall::Begin);
        Ok(())
    }
    async fn commit(&self, _session: SessionId) -> EngineResult<()> {
        self.log.lock().unwrap().push(DriverCall::Commit);
        Ok(())
    }
    async fn rollback(&self, _session: SessionId) -> EngineResult<()> {
        self.log.lock().unwrap().push(DriverCall::Rollback);
        Ok(())
    }
    fn supports_transactions(&self) -> bool {
        self.supports_tx
    }
    async fn describe_table(
        &self,
        _session: SessionId,
        _namespace: &Namespace,
        _table: &str,
    ) -> EngineResult<TableSchema> {
        Err(EngineError::not_supported("describe"))
    }
    async fn preview_table(
        &self,
        _session: SessionId,
        _namespace: &Namespace,
        _table: &str,
        _limit: u32,
    ) -> EngineResult<QueryResult> {
        Err(EngineError::not_supported("preview"))
    }
    async fn get_creation_options(&self, _session: SessionId) -> EngineResult<CreationOptions> {
        Ok(CreationOptions {
            charsets: Vec::new(),
        })
    }
}
