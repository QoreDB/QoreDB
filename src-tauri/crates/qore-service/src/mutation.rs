// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use qore_core::{DataEngine, SessionId};
use qore_drivers::session_manager::SessionManager;

use crate::interceptor::{
    Environment, InterceptorPipeline, QueryContext, QueryExecutionResult, SafetyAction,
};

const READ_ONLY_BLOCKED: &str = "Operation blocked: read-only mode";
const MUTATIONS_NOT_SUPPORTED: &str = "Mutations are not supported by this driver";
const DANGEROUS_BLOCKED: &str = "Dangerous query blocked: confirmation required";
const SAFETY_RULE_BLOCKED: &str = "Query blocked by safety rule";

fn map_environment(env: &str) -> Environment {
    match env {
        "production" => Environment::Production,
        "staging" => Environment::Staging,
        _ => Environment::Development,
    }
}

pub struct MutationPreflight {
    pub driver: Arc<dyn DataEngine>,
    pub context: QueryContext,
    pub environment: String,
    pub safety_warning: Option<String>,
}

pub async fn preflight(
    session_manager: &SessionManager,
    interceptor: &InterceptorPipeline,
    session: SessionId,
    session_id: &str,
    query_preview: &str,
    database: &str,
    acknowledged: bool,
) -> Result<MutationPreflight, String> {
    let read_only = session_manager
        .is_read_only(session)
        .await
        .map_err(|e| e.to_string())?;
    if read_only {
        return Err(READ_ONLY_BLOCKED.to_string());
    }

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;
    if !driver.capabilities().mutations {
        return Err(MUTATIONS_NOT_SUPPORTED.to_string());
    }

    let environment = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let interceptor_env = map_environment(&environment);

    let context = interceptor.build_context(
        session_id,
        query_preview,
        driver.driver_id(),
        interceptor_env,
        read_only,
        acknowledged,
        Some(database),
        None,
        true,
    );

    let safety_result = interceptor.pre_execute(&context);
    if !safety_result.allowed {
        interceptor.post_execute(
            &context,
            &QueryExecutionResult {
                success: false,
                error: safety_result.message.clone(),
                execution_time_ms: 0.0,
                row_count: None,
            },
            true,
            safety_result.triggered_rule.as_deref(),
        );

        let error_msg = match safety_result.action {
            SafetyAction::Block => format!(
                "{}: {}",
                SAFETY_RULE_BLOCKED,
                safety_result.message.unwrap_or_default()
            ),
            SafetyAction::RequireConfirmation => format!(
                "{}: {}",
                DANGEROUS_BLOCKED,
                safety_result.message.unwrap_or_default()
            ),
            SafetyAction::Warn => "Warning triggered".to_string(),
        };
        return Err(error_msg);
    }

    let safety_warning = if matches!(safety_result.action, SafetyAction::Warn) {
        safety_result.triggered_rule.clone()
    } else {
        None
    };

    Ok(MutationPreflight {
        driver,
        context,
        environment,
        safety_warning,
    })
}
