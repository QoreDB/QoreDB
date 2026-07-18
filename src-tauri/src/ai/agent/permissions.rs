// SPDX-License-Identifier: BUSL-1.1

//! Permission gate: classifies each tool call before execution.
//! Read-only exploration runs directly; writes need human confirmation in
//! dev/staging and are always blocked in production.

use qore_service::interceptor::Environment;
use qore_service::query::classify_mutation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    Auto,
    /// Suspend the loop and ask the user. `grant_key` is set when a
    /// session-lifetime "always allow" may be offered (dev/staging only).
    Confirm {
        reason: String,
        grant_key: Option<String>,
    },
    Block {
        reason: String,
    },
}

pub fn classify(
    tool_name: &str,
    query: Option<&str>,
    driver_id: &str,
    environment: Environment,
    connection_key: Option<&str>,
) -> Gate {
    match tool_name {
        "list_connections" | "list_namespaces" | "list_tables" | "describe_table" => Gate::Auto,
        "run_query" => match query.and_then(|q| classify_mutation(driver_id, q)) {
            Some(false) => Gate::Auto,
            // Writes must go through run_mutation; unparseable fails closed.
            Some(true) => Gate::Block {
                reason: "run_query is read-only; use run_mutation for writes".to_string(),
            },
            None => Gate::Block {
                reason: "Query could not be classified as read-only".to_string(),
            },
        },
        "run_mutation" => {
            if matches!(environment, Environment::Production) {
                return Gate::Block {
                    reason: "Writes initiated by the AI agent are always blocked in production"
                        .to_string(),
                };
            }
            Gate::Confirm {
                reason: "The agent wants to execute a write statement".to_string(),
                grant_key: connection_key.map(|k| format!("mutation|{k}")),
            }
        }
        // Federation is SELECT-only by construction, but always spans
        // connections: never remembered, always confirmed.
        "run_federated_query" => Gate::Confirm {
            reason: "The agent wants to run a federated query across connections".to_string(),
            grant_key: None,
        },
        other => Gate::Block {
            reason: format!("Unknown tool: {other}"),
        },
    }
}

/// Access to a connection outside the run's scope always requires
/// confirmation; "always allow" is only offered outside production.
pub fn classify_scope_access(
    display_name: &str,
    environment_label: &str,
    environment: Environment,
    connection_key: Option<&str>,
) -> Gate {
    Gate::Confirm {
        reason: format!(
            "The agent wants to access connection \"{display_name}\" ({environment_label})"
        ),
        grant_key: if matches!(environment, Environment::Production) {
            None
        } else {
            connection_key.map(|k| format!("scope|{k}"))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_via_run_query_is_auto() {
        let gate = classify(
            "run_query",
            Some("SELECT * FROM users"),
            "postgres",
            Environment::Production,
            Some("key"),
        );
        assert_eq!(gate, Gate::Auto);
    }

    #[test]
    fn mutation_via_run_query_is_blocked() {
        let gate = classify(
            "run_query",
            Some("DELETE FROM users"),
            "postgres",
            Environment::Development,
            Some("key"),
        );
        assert!(matches!(gate, Gate::Block { .. }));
    }

    #[test]
    fn unparseable_query_fails_closed() {
        let gate = classify(
            "run_query",
            Some("FROBNICATE ALL THE THINGS"),
            "postgres",
            Environment::Development,
            None,
        );
        assert!(matches!(gate, Gate::Block { .. }));
    }

    #[test]
    fn mutation_in_production_is_blocked_outright() {
        let gate = classify(
            "run_mutation",
            Some("UPDATE users SET active = false WHERE id = 1"),
            "postgres",
            Environment::Production,
            Some("key"),
        );
        assert!(matches!(gate, Gate::Block { .. }));
    }

    #[test]
    fn mutation_in_dev_requires_confirmation_with_grant() {
        let gate = classify(
            "run_mutation",
            Some("INSERT INTO t VALUES (1)"),
            "postgres",
            Environment::Development,
            Some("pg|localhost|5432|app|shop|development"),
        );
        match gate {
            Gate::Confirm { grant_key, .. } => {
                assert_eq!(
                    grant_key.as_deref(),
                    Some("mutation|pg|localhost|5432|app|shop|development")
                );
            }
            other => panic!("expected Confirm, got {other:?}"),
        }
    }

    #[test]
    fn mutation_in_staging_requires_confirmation() {
        let gate = classify(
            "run_mutation",
            Some("INSERT INTO t VALUES (1)"),
            "postgres",
            Environment::Staging,
            None,
        );
        assert!(matches!(gate, Gate::Confirm { .. }));
    }

    #[test]
    fn exploration_tools_are_auto() {
        for tool in ["list_namespaces", "list_tables", "describe_table"] {
            assert_eq!(
                classify(tool, None, "postgres", Environment::Production, None),
                Gate::Auto
            );
        }
    }

    #[test]
    fn unknown_tool_is_blocked() {
        assert!(matches!(
            classify("drop_database", None, "postgres", Environment::Development, None),
            Gate::Block { .. }
        ));
    }

    #[test]
    fn federated_query_always_confirms_without_grant() {
        match classify(
            "run_federated_query",
            Some("SELECT 1"),
            "postgres",
            Environment::Development,
            Some("key"),
        ) {
            Gate::Confirm { grant_key, .. } => assert!(grant_key.is_none()),
            other => panic!("expected Confirm, got {other:?}"),
        }
    }

    #[test]
    fn scope_access_to_production_cannot_be_remembered() {
        match classify_scope_access("prod-pg", "production", Environment::Production, Some("key"))
        {
            Gate::Confirm { grant_key, .. } => assert!(grant_key.is_none()),
            other => panic!("expected Confirm, got {other:?}"),
        }
    }

    #[test]
    fn scope_access_to_dev_offers_remember() {
        match classify_scope_access("local-pg", "development", Environment::Development, Some("k"))
        {
            Gate::Confirm { grant_key, .. } => assert_eq!(grant_key.as_deref(), Some("scope|k")),
            other => panic!("expected Confirm, got {other:?}"),
        }
    }
}
