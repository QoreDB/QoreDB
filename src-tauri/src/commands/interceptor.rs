// SPDX-License-Identifier: Apache-2.0

//! Commands for managing the Universal Query Interceptor system.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

use crate::interceptor::{
    AuditExportFormat, AuditLogEntry, AuditStats, Environment, FingerprintTrend, InterceptorConfig,
    ProfilingMetrics, QueryOperationType, SafetyRule, SlowQueryEntry, TrendFilter,
};

#[derive(Debug, Serialize)]
pub struct QueryTrendsResponse {
    pub success: bool,
    pub trends: Vec<FingerprintTrend>,
    pub error: Option<String>,
}

/// True iff the in-memory licence currently grants Pro-tier features; every
/// Pro entry point of this module checks it at call time, never at compile
/// time, so activating a licence takes effect without a rebuild.
async fn license_allows_pro(state: &State<'_, crate::SharedState>) -> bool {
    let tier = {
        let guard = state.lock().await;
        guard.license_manager.effective_status().tier
    };
    tier.includes(crate::license::status::LicenseTier::Pro)
}

/// Core: per-fingerprint trends from the audit file. Regressions ride along
/// only with a Pro licence.
#[tauri::command]
pub async fn get_query_trends(
    state: State<'_, crate::SharedState>,
    filter: TrendFilter,
) -> Result<QueryTrendsResponse, String> {
    let include_regressions = license_allows_pro(&state).await;
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };
    Ok(
        match interceptor.get_query_trends(&filter, include_regressions) {
            Ok(trends) => QueryTrendsResponse {
                success: true,
                trends,
                error: None,
            },
            Err(error) => QueryTrendsResponse {
                success: false,
                trends: Vec::new(),
                error: Some(error),
            },
        },
    )
}

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

#[derive(Debug, Deserialize)]
pub struct AuditFilter {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub environment: Option<Environment>,
    pub operation: Option<QueryOperationType>,
    pub success: Option<bool>,
    pub search: Option<String>,
    /// Restrict results to entries matching this fingerprint (Pro).
    pub fingerprint: Option<String>,
    /// `Some(true)` keeps only blocked entries; `Some(false)` excludes them.
    pub blocked: Option<bool>,
}

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

    let entries = if license_allows_pro(&state).await {
        interceptor.get_audit_entries(
            filter.limit.unwrap_or(100),
            filter.offset.unwrap_or(0),
            filter.environment,
            filter.operation,
            filter.success,
            filter.search.as_deref(),
            filter.fingerprint.as_deref(),
            filter.blocked,
        )
    } else {
        // Core: 50 entries, no filters.
        interceptor.get_audit_entries(
            filter.limit.unwrap_or(50).min(50),
            filter.offset.unwrap_or(0),
            None,
            None,
            None,
            None,
            None,
            None,
        )
    };

    Ok(AuditEntriesResponse {
        success: true,
        entries,
        error: None,
    })
}

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

/// Clears the audit log. Requires a one-shot confirmation token issued by
/// `request_confirmation_token("clear_audit_log")` to prevent drive-by IPC
/// calls from destroying the audit trail (SOC2 / RGPD impact).
#[tauri::command]
pub async fn clear_audit_log(
    state: State<'_, crate::SharedState>,
    confirmation_token: String,
) -> Result<GenericResponse, String> {
    let (interceptor, confirmation_tokens) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.interceptor),
            Arc::clone(&state.confirmation_tokens),
        )
    };

    confirmation_tokens.consume("clear_audit_log", &confirmation_token)?;
    interceptor.clear_audit();
    tracing::warn!("audit log cleared via clear_audit_log");

    Ok(GenericResponse {
        success: true,
        error: None,
    })
}

/// Exports the audit log (Pro only).
///
/// `format` selects the serialization (`json`, `jsonl`, `csv`). When
/// `from_disk` is `true`, the entire retained history is read from the rotated
/// JSONL file rather than just the in-memory cache — useful when retention
/// exceeds the cache size.
#[tauri::command]
pub async fn export_audit_log(
    state: State<'_, crate::SharedState>,
    format: Option<AuditExportFormat>,
    from_disk: Option<bool>,
) -> Result<ExportResponse, String> {
    if !license_allows_pro(&state).await {
        return Ok(ExportResponse {
            success: false,
            data: None,
            error: Some("Audit log export requires QoreDB Pro".into()),
        });
    }
    let interceptor = {
        let state = state.lock().await;
        Arc::clone(&state.interceptor)
    };

    let format = format.unwrap_or_default();
    let from_disk = from_disk.unwrap_or(false);

    match interceptor.export_audit_format(format, from_disk) {
        Ok(data) => Ok(ExportResponse {
            success: true,
            data: Some(data),
            error: None,
        }),
        Err(e) => Ok(ExportResponse {
            success: false,
            data: None,
            error: Some(e),
        }),
    }
}

#[tauri::command]
pub async fn get_profiling_metrics(
    state: State<'_, crate::SharedState>,
) -> Result<ProfilingMetricsResponse, String> {
    if !license_allows_pro(&state).await {
        return Ok(ProfilingMetricsResponse {
            success: false,
            metrics: None,
            error: Some("Query profiling requires QoreDB Pro".into()),
        });
    }
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

#[tauri::command]
pub async fn get_slow_queries(
    state: State<'_, crate::SharedState>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<SlowQueriesResponse, String> {
    if !license_allows_pro(&state).await {
        return Ok(SlowQueriesResponse {
            success: false,
            queries: vec![],
            error: Some("Query profiling requires QoreDB Pro".into()),
        });
    }
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

#[tauri::command]
pub async fn clear_slow_queries(
    state: State<'_, crate::SharedState>,
) -> Result<GenericResponse, String> {
    if !license_allows_pro(&state).await {
        return Ok(GenericResponse {
            success: false,
            error: Some("Query profiling requires QoreDB Pro".into()),
        });
    }
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

#[tauri::command]
pub async fn reset_profiling(
    state: State<'_, crate::SharedState>,
) -> Result<GenericResponse, String> {
    if !license_allows_pro(&state).await {
        return Ok(GenericResponse {
            success: false,
            error: Some("Query profiling requires QoreDB Pro".into()),
        });
    }
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

#[tauri::command]
pub async fn export_profiling(
    state: State<'_, crate::SharedState>,
) -> Result<ExportResponse, String> {
    if !license_allows_pro(&state).await {
        return Ok(ExportResponse {
            success: false,
            data: None,
            error: Some("Query profiling requires QoreDB Pro".into()),
        });
    }
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
#[tauri::command]
pub async fn add_safety_rule(
    state: State<'_, crate::SharedState>,
    rule: SafetyRule,
) -> Result<SafetyRulesResponse, String> {
    if !license_allows_pro(&state).await {
        return Ok(SafetyRulesResponse {
            success: false,
            rules: vec![],
            error: Some("Custom safety rules require QoreDB Pro".into()),
        });
    }
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

/// Updates an existing safety rule (Pro only)
#[tauri::command]
pub async fn update_safety_rule(
    state: State<'_, crate::SharedState>,
    rule: SafetyRule,
) -> Result<SafetyRulesResponse, String> {
    if !license_allows_pro(&state).await {
        return Ok(SafetyRulesResponse {
            success: false,
            rules: vec![],
            error: Some("Custom safety rules require QoreDB Pro".into()),
        });
    }
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

/// Removes a custom safety rule (Pro only)
#[tauri::command]
pub async fn remove_safety_rule(
    state: State<'_, crate::SharedState>,
    rule_id: String,
) -> Result<SafetyRulesResponse, String> {
    if !license_allows_pro(&state).await {
        return Ok(SafetyRulesResponse {
            success: false,
            rules: vec![],
            error: Some("Custom safety rules require QoreDB Pro".into()),
        });
    }
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
