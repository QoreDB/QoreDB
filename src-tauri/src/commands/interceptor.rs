// SPDX-License-Identifier: Apache-2.0

//! Interceptor Tauri Commands
//!
//! Commands for managing the Universal Query Interceptor system.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

use crate::interceptor::{
    AuditLogEntry, AuditStats, Environment, InterceptorConfig, ProfilingMetrics,
    QueryOperationType, SafetyRule, SlowQueryEntry,
};

// ==================== Response Types ====================

#[derive(Debug, Serialize)]
pub struct InterceptorConfigResponse {
    pub success: bool,
    pub config: Option<InterceptorConfig>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuditEntriesResponse {
    pub success: bool,
    pub entries: Vec<AuditLogEntry>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuditStatsResponse {
    pub success: bool,
    pub stats: Option<AuditStats>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProfilingMetricsResponse {
    pub success: bool,
    pub metrics: Option<ProfilingMetrics>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SlowQueriesResponse {
    pub success: bool,
    pub queries: Vec<SlowQueryEntry>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SafetyRulesResponse {
    pub success: bool,
    pub rules: Vec<SafetyRule>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GenericResponse {
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExportResponse {
    pub success: bool,
    pub data: Option<String>,
    pub error: Option<String>,
}

// ==================== Configuration Commands ====================

/// Gets the interceptor configuration
#[tauri::command]
pub async fn get_interceptor_config(
    state: State<'_, crate::SharedState>,
) -> Result<InterceptorConfigResponse, String> {
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };

    let config = interceptor.get_config();

    Ok(InterceptorConfigResponse {
        success: true,
        config: Some(config),
        error: None,
    })
}

/// Updates the interceptor configuration
#[tauri::command]
pub async fn update_interceptor_config(
    state: State<'_, crate::SharedState>,
    config: InterceptorConfig,
) -> Result<InterceptorConfigResponse, String> {
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };

    match interceptor.update_config(config) {
        Ok(()) => {
            let updated = interceptor.get_config();
            Ok(InterceptorConfigResponse {
                success: true,
                config: Some(updated),
                error: None,
            })
        }
        Err(e) => Ok(InterceptorConfigResponse {
            success: false,
            config: None,
            error: Some(e),
        }),
    }
}

// ==================== Audit Commands ====================

/// Filter parameters for audit log queries
#[derive(Debug, Deserialize)]
pub struct AuditFilter {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub environment: Option<Environment>,
    pub operation: Option<QueryOperationType>,
    pub success: Option<bool>,
    pub search: Option<String>,
}

/// Gets audit log entries with optional filtering
/// Core: limited to 50 entries, no advanced filters. Pro: unlimited.
#[tauri::command]
pub async fn get_audit_entries(
    state: State<'_, crate::SharedState>,
    filter: AuditFilter,
) -> Result<AuditEntriesResponse, String> {
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };

    #[cfg(feature = "pro")]
    let entries = interceptor.get_audit_entries(
        filter.limit.unwrap_or(100),
        filter.offset.unwrap_or(0),
        filter.environment,
        filter.operation,
        filter.success,
        filter.search.as_deref(),
    );

    #[cfg(not(feature = "pro"))]
    let entries = interceptor.get_audit_entries(
        filter.limit.unwrap_or(50).min(50),
        filter.offset.unwrap_or(0),
        None,  // No environment filter in Core
        None,  // No operation filter in Core
        None,  // No success filter in Core
        None,  // No search in Core
    );

    Ok(AuditEntriesResponse {
        success: true,
        entries,
        error: None,
    })
}

/// Gets audit log statistics
#[tauri::command]
pub async fn get_audit_stats(
    state: State<'_, crate::SharedState>,
) -> Result<AuditStatsResponse, String> {
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };

    let stats = interceptor.get_audit_stats();

    Ok(AuditStatsResponse {
        success: true,
        stats: Some(stats),
        error: None,
    })
}

/// Clears the audit log
#[tauri::command]
pub async fn clear_audit_log(
    state: State<'_, crate::SharedState>,
) -> Result<GenericResponse, String> {
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };

    interceptor.clear_audit();

    Ok(GenericResponse {
        success: true,
        error: None,
    })
}

/// Exports the audit log as JSON (Pro only)
#[cfg(feature = "pro")]
#[tauri::command]
pub async fn export_audit_log(
    state: State<'_, crate::SharedState>,
) -> Result<ExportResponse, String> {
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };

    let data = interceptor.export_audit();

    Ok(ExportResponse {
        success: true,
        data: Some(data),
        error: None,
    })
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn export_audit_log(
    _state: State<'_, crate::SharedState>,
) -> Result<ExportResponse, String> {
    Ok(ExportResponse {
        success: false,
        data: None,
        error: Some("Audit log export requires QoreDB Pro".into()),
    })
}

// ==================== Profiling Commands (Pro only) ====================

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn get_profiling_metrics(
    state: State<'_, crate::SharedState>,
) -> Result<ProfilingMetricsResponse, String> {
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };
    let metrics = interceptor.get_profiling_metrics();
    Ok(ProfilingMetricsResponse {
        success: true,
        metrics: Some(metrics),
        error: None,
    })
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn get_profiling_metrics(
    _state: State<'_, crate::SharedState>,
) -> Result<ProfilingMetricsResponse, String> {
    Ok(ProfilingMetricsResponse {
        success: false,
        metrics: None,
        error: Some("Query profiling requires QoreDB Pro".into()),
    })
}

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn get_slow_queries(
    state: State<'_, crate::SharedState>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<SlowQueriesResponse, String> {
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };
    let queries = interceptor.get_slow_queries(limit.unwrap_or(50), offset.unwrap_or(0));
    Ok(SlowQueriesResponse {
        success: true,
        queries,
        error: None,
    })
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn get_slow_queries(
    _state: State<'_, crate::SharedState>,
    _limit: Option<usize>,
    _offset: Option<usize>,
) -> Result<SlowQueriesResponse, String> {
    Ok(SlowQueriesResponse {
        success: false,
        queries: vec![],
        error: Some("Query profiling requires QoreDB Pro".into()),
    })
}

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn clear_slow_queries(
    state: State<'_, crate::SharedState>,
) -> Result<GenericResponse, String> {
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };
    interceptor.clear_slow_queries();
    Ok(GenericResponse {
        success: true,
        error: None,
    })
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn clear_slow_queries(
    _state: State<'_, crate::SharedState>,
) -> Result<GenericResponse, String> {
    Ok(GenericResponse {
        success: false,
        error: Some("Query profiling requires QoreDB Pro".into()),
    })
}

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn reset_profiling(
    state: State<'_, crate::SharedState>,
) -> Result<GenericResponse, String> {
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };
    interceptor.reset_profiling();
    Ok(GenericResponse {
        success: true,
        error: None,
    })
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn reset_profiling(
    _state: State<'_, crate::SharedState>,
) -> Result<GenericResponse, String> {
    Ok(GenericResponse {
        success: false,
        error: Some("Query profiling requires QoreDB Pro".into()),
    })
}

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn export_profiling(
    state: State<'_, crate::SharedState>,
) -> Result<ExportResponse, String> {
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };
    let data = interceptor.export_profiling();
    Ok(ExportResponse {
        success: true,
        data: Some(data),
        error: None,
    })
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn export_profiling(
    _state: State<'_, crate::SharedState>,
) -> Result<ExportResponse, String> {
    Ok(ExportResponse {
        success: false,
        data: None,
        error: Some("Query profiling requires QoreDB Pro".into()),
    })
}

// ==================== Safety Rules Commands ====================

/// Gets all safety rules (built-in + custom)
#[tauri::command]
pub async fn get_safety_rules(
    state: State<'_, crate::SharedState>,
) -> Result<SafetyRulesResponse, String> {
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };

    let rules = interceptor.get_safety_rules();

    Ok(SafetyRulesResponse {
        success: true,
        rules,
        error: None,
    })
}

/// Adds a custom safety rule (Pro only)
#[cfg(feature = "pro")]
#[tauri::command]
pub async fn add_safety_rule(
    state: State<'_, crate::SharedState>,
    rule: SafetyRule,
) -> Result<SafetyRulesResponse, String> {
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };

    match interceptor.add_safety_rule(rule) {
        Ok(()) => {
            let rules = interceptor.get_safety_rules();
            Ok(SafetyRulesResponse {
                success: true,
                rules,
                error: None,
            })
        }
        Err(e) => Ok(SafetyRulesResponse {
            success: false,
            rules: vec![],
            error: Some(e),
        }),
    }
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn add_safety_rule(
    _state: State<'_, crate::SharedState>,
    _rule: SafetyRule,
) -> Result<SafetyRulesResponse, String> {
    Ok(SafetyRulesResponse {
        success: false,
        rules: vec![],
        error: Some("Custom safety rules require QoreDB Pro".into()),
    })
}

/// Updates an existing safety rule (Pro only)
#[cfg(feature = "pro")]
#[tauri::command]
pub async fn update_safety_rule(
    state: State<'_, crate::SharedState>,
    rule: SafetyRule,
) -> Result<SafetyRulesResponse, String> {
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };

    match interceptor.update_safety_rule(rule) {
        Ok(()) => {
            let rules = interceptor.get_safety_rules();
            Ok(SafetyRulesResponse {
                success: true,
                rules,
                error: None,
            })
        }
        Err(e) => Ok(SafetyRulesResponse {
            success: false,
            rules: vec![],
            error: Some(e),
        }),
    }
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn update_safety_rule(
    _state: State<'_, crate::SharedState>,
    _rule: SafetyRule,
) -> Result<SafetyRulesResponse, String> {
    Ok(SafetyRulesResponse {
        success: false,
        rules: vec![],
        error: Some("Custom safety rules require QoreDB Pro".into()),
    })
}

/// Removes a custom safety rule (Pro only)
#[cfg(feature = "pro")]
#[tauri::command]
pub async fn remove_safety_rule(
    state: State<'_, crate::SharedState>,
    rule_id: String,
) -> Result<SafetyRulesResponse, String> {
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };

    match interceptor.remove_safety_rule(&rule_id) {
        Ok(()) => {
            let rules = interceptor.get_safety_rules();
            Ok(SafetyRulesResponse {
                success: true,
                rules,
                error: None,
            })
        }
        Err(e) => Ok(SafetyRulesResponse {
            success: false,
            rules: vec![],
            error: Some(e),
        }),
    }
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn remove_safety_rule(
    _state: State<'_, crate::SharedState>,
    _rule_id: String,
) -> Result<SafetyRulesResponse, String> {
    Ok(SafetyRulesResponse {
        success: false,
        rules: vec![],
        error: Some("Custom safety rules require QoreDB Pro".into()),
    })
}
