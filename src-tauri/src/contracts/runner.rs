// SPDX-License-Identifier: BUSL-1.1

//! Contract execution orchestration.
//!
//! Given an active `DataEngine` + session and a parsed `Contract`, walk every
//! rule, generate the dialect-specific SQL via `contracts::sql`, run it, and
//! aggregate the results into a [`ContractRun`].
//!
//! The runner is dialect-aware but driver-agnostic: it relies on the
//! [`DataEngine`] trait for execution, so it works for every SQL backend
//! supported by QoreDB (Postgres family, MySQL/MariaDB, SQLite, DuckDB,
//! SQL Server, ClickHouse).

use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use qore_core::traits::DataEngine;
use qore_core::types::{QueryId, QueryResult, Row, SessionId, Value};

use super::events::{ContractEventSink, ContractRunEvent};
use super::sql::dialect::Dialect;
use super::sql::{build_rule_sql, RuleSql, RuleSqlKind, SqlBuildError, DEFAULT_SAMPLE_LIMIT};
use super::{Contract, ContractRun, Rule, RuleResult, RuleStatus};

/// Configuration knobs surfaced through the Tauri command. Sensible defaults
/// keep `run_contract` ergonomic to call from tests.
#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    pub sample_limit: u32,
    /// When false, samples are never collected even on failing rules. The UI
    /// can choose to skip collection on very wide tables.
    pub collect_samples: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            sample_limit: DEFAULT_SAMPLE_LIMIT,
            collect_samples: true,
        }
    }
}

/// Errors returned before the run even starts. Once started, every rule
/// failure is captured inside the [`ContractRun`] instead of bubbling up.
#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("unknown driver dialect: {0}")]
    UnknownDialect(String),
}

/// Executes a contract end-to-end. Emits streaming events through `sink`
/// (use [`super::events::NoopSink`] when not needed) and returns the
/// aggregated [`ContractRun`] when finished.
pub async fn run_contract(
    driver: Arc<dyn DataEngine>,
    session: SessionId,
    connection_id: String,
    contract: &Contract,
    options: RunOptions,
    sink: &dyn ContractEventSink,
) -> Result<ContractRun, RunnerError> {
    let run_id = uuid::Uuid::new_v4().to_string();
    let contract_id = contract.name.clone();
    let started_at = Utc::now();
    let run_started = Instant::now();
    let total_rules = contract.rules.len() as u32;

    let dialect = match Dialect::from_driver_id(driver.driver_id()) {
        Some(d) => d,
        None => {
            sink.emit(ContractRunEvent::Failed {
                run_id: run_id.clone(),
                contract_id: contract_id.clone(),
                error: format!(
                    "driver {} is not supported by Data Contracts",
                    driver.driver_id()
                ),
            });
            return Err(RunnerError::UnknownDialect(driver.driver_id().to_string()));
        }
    };

    sink.emit(ContractRunEvent::Started {
        run_id: run_id.clone(),
        contract_id: contract_id.clone(),
        contract_name: contract.name.clone(),
        rules_total: total_rules,
    });

    let mut results = Vec::with_capacity(contract.rules.len());

    for (idx, rule) in contract.rules.iter().enumerate() {
        let index = idx as u32 + 1;
        sink.emit(ContractRunEvent::RuleStarted {
            run_id: run_id.clone(),
            contract_id: contract_id.clone(),
            rule_id: rule.id().to_string(),
            rule_type: rule.rule_type().to_string(),
            index,
            total: total_rules,
        });

        let result =
            evaluate_rule(driver.as_ref(), session, &contract, rule, dialect, options).await;

        sink.emit(ContractRunEvent::Progress {
            run_id: run_id.clone(),
            contract_id: contract_id.clone(),
            result: result.clone(),
            index,
            total: total_rules,
        });

        results.push(result);
    }

    let mut pass_count = 0u32;
    let mut fail_count = 0u32;
    let mut error_count = 0u32;
    for r in &results {
        match r.status {
            RuleStatus::Pass => pass_count += 1,
            RuleStatus::Fail => fail_count += 1,
            RuleStatus::Error => error_count += 1,
            RuleStatus::Skipped => {}
        }
    }

    let finished_at = Utc::now();
    let duration_ms = run_started.elapsed().as_millis() as u64;
    let run = ContractRun {
        contract_id: contract_id.clone(),
        contract_name: contract.name.clone(),
        connection_id,
        started_at: started_at.to_rfc3339(),
        finished_at: finished_at.to_rfc3339(),
        duration_ms,
        pass_count,
        fail_count,
        error_count,
        results,
    };

    sink.emit(ContractRunEvent::Completed {
        run_id,
        run: run.clone(),
    });

    Ok(run)
}

async fn evaluate_rule(
    driver: &dyn DataEngine,
    session: SessionId,
    contract: &Contract,
    rule: &Rule,
    dialect: Dialect,
    options: RunOptions,
) -> RuleResult {
    let started = Instant::now();
    let rule_type = rule.rule_type().to_string();
    let rule_id = rule.id().to_string();

    if !rule.enabled() {
        return RuleResult {
            id: rule_id,
            rule_type,
            status: RuleStatus::Skipped,
            violations_count: None,
            metric: None,
            samples: None,
            duration_ms: started.elapsed().as_millis() as u64,
            error: Some("rule disabled".into()),
        };
    }

    let sql = match build_rule_sql(rule, &contract.target, dialect, options.sample_limit) {
        Ok(s) => s,
        Err(SqlBuildError::UnsupportedOnDialect(rule_type_str, driver_name)) => {
            return RuleResult {
                id: rule_id,
                rule_type,
                status: RuleStatus::Skipped,
                violations_count: None,
                metric: None,
                samples: None,
                duration_ms: started.elapsed().as_millis() as u64,
                error: Some(format!("{rule_type_str} is not supported on {driver_name}")),
            };
        }
        Err(err) => {
            return RuleResult {
                id: rule_id,
                rule_type,
                status: RuleStatus::Error,
                violations_count: None,
                metric: None,
                samples: None,
                duration_ms: started.elapsed().as_millis() as u64,
                error: Some(err.to_string()),
            };
        }
    };

    execute_rule_sql(
        driver, session, rule, &rule_id, &rule_type, sql, options, started,
    )
    .await
}

async fn execute_rule_sql(
    driver: &dyn DataEngine,
    session: SessionId,
    rule: &Rule,
    rule_id: &str,
    rule_type: &str,
    sql: RuleSql,
    options: RunOptions,
    started: Instant,
) -> RuleResult {
    let metric_result = match driver
        .execute(session, &sql.metric_query, QueryId::new())
        .await
    {
        Ok(r) => r,
        Err(err) => {
            return RuleResult {
                id: rule_id.to_string(),
                rule_type: rule_type.to_string(),
                status: RuleStatus::Error,
                violations_count: None,
                metric: None,
                samples: None,
                duration_ms: started.elapsed().as_millis() as u64,
                error: Some(err.sanitized_message()),
            };
        }
    };

    let parsed = parse_metric_row(&metric_result, sql.kind);
    let (status, violations_count, metric) = match parsed {
        Some(out) => evaluate_status(rule, sql.kind, out),
        None => {
            return RuleResult {
                id: rule_id.to_string(),
                rule_type: rule_type.to_string(),
                status: RuleStatus::Error,
                violations_count: None,
                metric: None,
                samples: None,
                duration_ms: started.elapsed().as_millis() as u64,
                error: Some("metric query returned no rows".into()),
            };
        }
    };

    let samples = if matches!(status, RuleStatus::Fail) && options.collect_samples {
        match &sql.samples_query {
            Some(q) => fetch_samples(driver, session, q).await,
            None => None,
        }
    } else {
        None
    };

    RuleResult {
        id: rule_id.to_string(),
        rule_type: rule_type.to_string(),
        status,
        violations_count,
        metric,
        samples,
        duration_ms: started.elapsed().as_millis() as u64,
        error: None,
    }
}

/// Raw numeric output of the metric query for a given `RuleSqlKind`. Stored as
/// `f64` so we can mix integer counts and percentages without loss for the
/// magnitudes contracts actually observe.
#[derive(Debug, Clone, Copy)]
enum MetricOutput {
    Violations { violations: u64, total: Option<u64> },
    Single { value: f64 },
}

fn parse_metric_row(result: &QueryResult, kind: RuleSqlKind) -> Option<MetricOutput> {
    let row = result.rows.first()?;
    match kind {
        RuleSqlKind::ViolationsCount => {
            let violations = row.values.first().and_then(value_as_u64)?;
            let total = row.values.get(1).and_then(value_as_u64);
            Some(MetricOutput::Violations { violations, total })
        }
        RuleSqlKind::SingleMetric => {
            let value = row.values.first().and_then(value_as_f64)?;
            Some(MetricOutput::Single { value })
        }
        RuleSqlKind::CustomViolations => {
            let violations = row.values.first().and_then(value_as_u64)?;
            Some(MetricOutput::Violations {
                violations,
                total: None,
            })
        }
    }
}

fn evaluate_status(
    rule: &Rule,
    _kind: RuleSqlKind,
    metric: MetricOutput,
) -> (RuleStatus, Option<u64>, Option<f64>) {
    match metric {
        MetricOutput::Violations { violations, total } => {
            let status = if violations == 0 {
                RuleStatus::Pass
            } else {
                RuleStatus::Fail
            };
            let pct = total
                .filter(|t| *t > 0)
                .map(|t| (t - violations) as f64 * 100.0 / t as f64);
            (status, Some(violations), pct)
        }
        MetricOutput::Single { value } => {
            let status = evaluate_single_metric(rule, value);
            (status, None, Some(value))
        }
    }
}

fn evaluate_single_metric(rule: &Rule, value: f64) -> RuleStatus {
    match rule {
        Rule::NotNullPct {
            threshold_min_pct, ..
        } => {
            if value >= *threshold_min_pct {
                RuleStatus::Pass
            } else {
                RuleStatus::Fail
            }
        }
        Rule::RowCount { min, max, .. } => check_range_i64(value, *min, *max),
        Rule::DistinctCount { min, max, .. } => check_range_i64(value, *min, *max),
        _ => RuleStatus::Error,
    }
}

fn check_range_i64(value: f64, min: Option<i64>, max: Option<i64>) -> RuleStatus {
    if let Some(min) = min {
        if value < min as f64 {
            return RuleStatus::Fail;
        }
    }
    if let Some(max) = max {
        if value > max as f64 {
            return RuleStatus::Fail;
        }
    }
    RuleStatus::Pass
}

async fn fetch_samples(
    driver: &dyn DataEngine,
    session: SessionId,
    query: &str,
) -> Option<Vec<serde_json::Value>> {
    let result = driver.execute(session, query, QueryId::new()).await.ok()?;
    let columns: Vec<&str> = result.columns.iter().map(|c| c.name.as_str()).collect();
    let samples = result
        .rows
        .iter()
        .map(|row| row_to_json(&columns, row))
        .collect();
    Some(samples)
}

fn row_to_json(columns: &[&str], row: &Row) -> serde_json::Value {
    let mut map = serde_json::Map::with_capacity(columns.len());
    for (i, col) in columns.iter().enumerate() {
        let v = row
            .values
            .get(i)
            .map(value_to_json)
            .unwrap_or(serde_json::Value::Null);
        map.insert((*col).to_string(), v);
    }
    serde_json::Value::Object(map)
}

fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(i) => serde_json::Value::from(*i),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::Text(s) => serde_json::Value::String(s.clone()),
        Value::Bytes(b) => {
            use base64::Engine as _;
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(b))
        }
        Value::Json(j) => j.clone(),
        Value::Array(arr) => serde_json::Value::Array(arr.iter().map(value_to_json).collect()),
    }
}

fn value_as_u64(v: &Value) -> Option<u64> {
    match v {
        Value::Int(i) => u64::try_from(*i).ok(),
        Value::Float(f) if f.is_finite() && *f >= 0.0 => Some(f.round() as u64),
        Value::Text(s) => s.parse::<u64>().ok(),
        _ => None,
    }
}

fn value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        Value::Text(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::events::testing::RecordingSink;
    use crate::contracts::{Contract, ContractTarget, ForeignKeyReference, Rule};
    use async_trait::async_trait;
    use qore_core::error::{EngineError, EngineResult};
    use qore_core::traits::DataEngine;
    use qore_core::types::{
        CollectionList, CollectionListOptions, ColumnInfo, ConnectionConfig, CreationOptions,
        Namespace, QueryId, QueryResult, Row, SessionId, TableSchema, Value,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Mock driver that returns pre-canned results keyed by the SQL fragment
    /// looked up in `responses` (substring match).
    struct MockDriver {
        driver_id: &'static str,
        responses: Mutex<HashMap<String, EngineResult<QueryResult>>>,
        calls: Mutex<Vec<String>>,
    }

    impl MockDriver {
        fn new(driver_id: &'static str) -> Self {
            Self {
                driver_id,
                responses: Mutex::new(HashMap::new()),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn add(&self, needle: &str, result: QueryResult) {
            self.responses
                .lock()
                .unwrap()
                .insert(needle.to_string(), Ok(result));
        }

        fn add_err(&self, needle: &str, msg: &str) {
            self.responses
                .lock()
                .unwrap()
                .insert(needle.to_string(), Err(EngineError::internal(msg)));
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
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
            let map = self.responses.lock().unwrap();
            for (needle, response) in map.iter() {
                if query.contains(needle) {
                    return match response {
                        Ok(r) => Ok(r.clone()),
                        Err(e) => Err(EngineError::internal(e.to_string())),
                    };
                }
            }
            Err(EngineError::internal(format!("no mock for: {query}")))
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

    fn target() -> ContractTarget {
        ContractTarget {
            connection: "c1".into(),
            schema: Some("public".into()),
            table: "orders".into(),
        }
    }

    fn single_row(col: &str, value: Value) -> QueryResult {
        QueryResult {
            columns: vec![ColumnInfo {
                name: col.into(),
                data_type: "int".into(),
                nullable: false,
            }],
            rows: vec![Row {
                values: vec![value],
            }],
            affected_rows: None,
            execution_time_ms: 0.0,
        }
    }

    fn two_col_row(a: Value, b: Value) -> QueryResult {
        QueryResult {
            columns: vec![
                ColumnInfo {
                    name: "violations".into(),
                    data_type: "int".into(),
                    nullable: false,
                },
                ColumnInfo {
                    name: "total".into(),
                    data_type: "int".into(),
                    nullable: false,
                },
            ],
            rows: vec![Row { values: vec![a, b] }],
            affected_rows: None,
            execution_time_ms: 0.0,
        }
    }

    #[tokio::test]
    async fn run_passes_when_row_count_in_range() {
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.add(
            "count(*) AS metric_value",
            single_row("metric_value", Value::Int(50)),
        );

        let contract = Contract {
            name: "c1".into(),
            version: 1,
            description: None,
            target: target(),
            rules: vec![Rule::RowCount {
                id: "r1".into(),
                description: None,
                severity: None,
                enabled: None,
                min: Some(10),
                max: Some(100),
            }],
        };

        let sink = RecordingSink::default();
        let run = run_contract(
            driver.clone(),
            SessionId::new(),
            "conn-1".into(),
            &contract,
            RunOptions::default(),
            &sink,
        )
        .await
        .unwrap();

        assert_eq!(run.pass_count, 1);
        assert_eq!(run.fail_count, 0);
        assert_eq!(run.results[0].status, RuleStatus::Pass);
        assert_eq!(run.results[0].metric, Some(50.0));
    }

    #[tokio::test]
    async fn run_fails_when_row_count_below_min() {
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.add(
            "count(*) AS metric_value",
            single_row("metric_value", Value::Int(5)),
        );

        let contract = Contract {
            name: "c1".into(),
            version: 1,
            description: None,
            target: target(),
            rules: vec![Rule::RowCount {
                id: "r1".into(),
                description: None,
                severity: None,
                enabled: None,
                min: Some(10),
                max: Some(100),
            }],
        };

        let run = run_contract(
            driver,
            SessionId::new(),
            "conn-1".into(),
            &contract,
            RunOptions::default(),
            &super::super::events::NoopSink,
        )
        .await
        .unwrap();

        assert_eq!(run.fail_count, 1);
        assert_eq!(run.results[0].status, RuleStatus::Fail);
    }

    #[tokio::test]
    async fn run_collects_samples_on_failure() {
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.add(
            ") AS violations,",
            two_col_row(Value::Int(3), Value::Int(100)),
        );
        driver.add(
            "SELECT * FROM",
            QueryResult {
                columns: vec![ColumnInfo {
                    name: "email".into(),
                    data_type: "text".into(),
                    nullable: true,
                }],
                rows: vec![
                    Row {
                        values: vec![Value::Null],
                    },
                    Row {
                        values: vec![Value::Null],
                    },
                ],
                affected_rows: None,
                execution_time_ms: 0.0,
            },
        );

        let contract = Contract {
            name: "c1".into(),
            version: 1,
            description: None,
            target: target(),
            rules: vec![Rule::NotEmpty {
                id: "r1".into(),
                description: None,
                severity: None,
                enabled: None,
                column: "email".into(),
            }],
        };

        let run = run_contract(
            driver,
            SessionId::new(),
            "conn-1".into(),
            &contract,
            RunOptions::default(),
            &super::super::events::NoopSink,
        )
        .await
        .unwrap();

        assert_eq!(run.results[0].status, RuleStatus::Fail);
        assert_eq!(run.results[0].violations_count, Some(3));
        let samples = run.results[0].samples.as_ref().unwrap();
        assert_eq!(samples.len(), 2);
    }

    #[tokio::test]
    async fn run_skips_unsupported_rule() {
        let driver = Arc::new(MockDriver::new("sqlite"));

        let contract = Contract {
            name: "c1".into(),
            version: 1,
            description: None,
            target: target(),
            rules: vec![Rule::RegexMatch {
                id: "r1".into(),
                description: None,
                severity: None,
                enabled: None,
                column: "email".into(),
                pattern: "^.+@.+$".into(),
            }],
        };

        let run = run_contract(
            driver.clone(),
            SessionId::new(),
            "conn-1".into(),
            &contract,
            RunOptions::default(),
            &super::super::events::NoopSink,
        )
        .await
        .unwrap();

        assert_eq!(run.results[0].status, RuleStatus::Skipped);
        assert!(run.results[0]
            .error
            .as_deref()
            .unwrap()
            .contains("regex_match"));
        // No execute call should have happened.
        assert!(driver.calls().is_empty());
    }

    #[tokio::test]
    async fn run_returns_error_on_execute_failure() {
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.add_err("count(*) AS metric_value", "boom");

        let contract = Contract {
            name: "c1".into(),
            version: 1,
            description: None,
            target: target(),
            rules: vec![Rule::RowCount {
                id: "r1".into(),
                description: None,
                severity: None,
                enabled: None,
                min: None,
                max: Some(10),
            }],
        };

        let run = run_contract(
            driver,
            SessionId::new(),
            "conn-1".into(),
            &contract,
            RunOptions::default(),
            &super::super::events::NoopSink,
        )
        .await
        .unwrap();

        assert_eq!(run.error_count, 1);
        assert_eq!(run.results[0].status, RuleStatus::Error);
        assert!(run.results[0].error.as_deref().unwrap().contains("boom"));
    }

    #[tokio::test]
    async fn run_skips_disabled_rules() {
        let driver = Arc::new(MockDriver::new("postgres"));

        let contract = Contract {
            name: "c1".into(),
            version: 1,
            description: None,
            target: target(),
            rules: vec![Rule::RowCount {
                id: "r1".into(),
                description: None,
                severity: None,
                enabled: Some(false),
                min: Some(10),
                max: Some(20),
            }],
        };

        let run = run_contract(
            driver.clone(),
            SessionId::new(),
            "conn-1".into(),
            &contract,
            RunOptions::default(),
            &super::super::events::NoopSink,
        )
        .await
        .unwrap();

        assert_eq!(run.results[0].status, RuleStatus::Skipped);
        assert!(driver.calls().is_empty());
    }

    #[tokio::test]
    async fn run_fails_on_unknown_dialect() {
        let driver = Arc::new(MockDriver::new("mongodb"));
        let contract = Contract {
            name: "c1".into(),
            version: 1,
            description: None,
            target: target(),
            rules: vec![Rule::RowCount {
                id: "r1".into(),
                description: None,
                severity: None,
                enabled: None,
                min: None,
                max: None,
            }],
        };

        let sink = RecordingSink::default();
        let err = run_contract(
            driver,
            SessionId::new(),
            "conn-1".into(),
            &contract,
            RunOptions::default(),
            &sink,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, RunnerError::UnknownDialect(_)));
        let events = sink.events();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], ContractRunEvent::Failed { .. }));
    }

    #[tokio::test]
    async fn run_emits_progress_for_every_rule() {
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.add(
            "count(*) AS metric_value",
            single_row("metric_value", Value::Int(50)),
        );

        let contract = Contract {
            name: "c1".into(),
            version: 1,
            description: None,
            target: target(),
            rules: vec![
                Rule::RowCount {
                    id: "r1".into(),
                    description: None,
                    severity: None,
                    enabled: None,
                    min: Some(10),
                    max: Some(100),
                },
                Rule::RowCount {
                    id: "r2".into(),
                    description: None,
                    severity: None,
                    enabled: None,
                    min: Some(10),
                    max: Some(100),
                },
            ],
        };

        let sink = RecordingSink::default();
        run_contract(
            driver,
            SessionId::new(),
            "conn-1".into(),
            &contract,
            RunOptions::default(),
            &sink,
        )
        .await
        .unwrap();

        let events = sink.events();
        let started = events
            .iter()
            .filter(|e| matches!(e, ContractRunEvent::Started { .. }))
            .count();
        let rule_started = events
            .iter()
            .filter(|e| matches!(e, ContractRunEvent::RuleStarted { .. }))
            .count();
        let progress = events
            .iter()
            .filter(|e| matches!(e, ContractRunEvent::Progress { .. }))
            .count();
        let completed = events
            .iter()
            .filter(|e| matches!(e, ContractRunEvent::Completed { .. }))
            .count();

        assert_eq!(started, 1);
        assert_eq!(rule_started, 2);
        assert_eq!(progress, 2);
        assert_eq!(completed, 1);
        // Ensure FK reference type stays exercised so an unused import never lands.
        let _: Option<ForeignKeyReference> = None;
    }
}
