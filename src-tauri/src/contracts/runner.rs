// SPDX-License-Identifier: BUSL-1.1

//! Desktop re-export of `qore_service::contracts::runner` and mock-driver unit tests.

pub use qore_service::contracts::runner::*;

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::contracts::events::{ContractEventSink, ContractRunEvent};
    use crate::contracts::{Contract, ContractTarget, ForeignKeyReference, Rule, RuleStatus};
    use crate::engine::testing::MockDriver;
    use qore_core::types::{ColumnInfo, QueryResult, Row, SessionId, Value};

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<ContractRunEvent>>,
    }

    impl RecordingSink {
        fn events(&self) -> Vec<ContractRunEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    impl ContractEventSink for RecordingSink {
        fn emit(&self, event: ContractRunEvent) {
            self.events.lock().unwrap().push(event);
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
                masked: false,
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
                    masked: false,
                },
                ColumnInfo {
                    name: "total".into(),
                    data_type: "int".into(),
                    nullable: false,
                    masked: false,
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
                    masked: false,
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
        assert!(
            run.results[0]
                .error
                .as_deref()
                .unwrap()
                .contains("regex_match")
        );
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
