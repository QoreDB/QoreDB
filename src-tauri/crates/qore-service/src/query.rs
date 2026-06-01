// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use qore_core::{
    DataEngine, Namespace, PaginatedQueryResult, QueryResult, SessionId, TableQueryOptions,
    TableSchema,
};
use qore_drivers::query_manager::QueryManager;
use qore_drivers::session_manager::SessionManager;
use qore_drivers::{mongo_safety, redis_safety};
use qore_sql::safety as sql_safety;

use crate::cache::QueryCache;
use crate::error::ServiceError;
use crate::governance;
use crate::interceptor::{
    Environment, InterceptorPipeline, QueryContext, QueryExecutionResult, SafetyAction,
};
use crate::policy::SafetyPolicy;
use crate::ratelimit::QueryRateLimiter;
use crate::virtual_relations::VirtualRelationStore;

const READ_ONLY_BLOCKED: &str = "Operation blocked: read-only mode";
const DANGEROUS_BLOCKED: &str = "Dangerous query blocked: confirmation required";
const DANGEROUS_BLOCKED_POLICY: &str = "Dangerous query blocked by policy";
const SQL_PARSE_BLOCKED: &str = "Operation blocked: SQL parser could not classify the query";
const RATE_LIMIT_BLOCKED: &str =
    "Operation blocked: query rate limit exceeded — too many queries in a short time";
const SAFETY_RULE_BLOCKED: &str = "Query blocked by safety rule";

fn is_mongo_mutation(query: &str) -> bool {
    matches!(
        mongo_safety::classify(query),
        mongo_safety::MongoQueryClass::Mutation | mongo_safety::MongoQueryClass::Unknown
    )
}

fn is_redis_mutation(query: &str) -> bool {
    matches!(
        redis_safety::classify(query),
        redis_safety::RedisQueryClass::Mutation
            | redis_safety::RedisQueryClass::Dangerous
            | redis_safety::RedisQueryClass::Unknown
    )
}

fn is_redis_dangerous(query: &str) -> bool {
    matches!(
        redis_safety::classify(query),
        redis_safety::RedisQueryClass::Dangerous
    )
}

fn map_environment(env: &str) -> Environment {
    match env {
        "production" => Environment::Production,
        "staging" => Environment::Staging,
        _ => Environment::Development,
    }
}

pub async fn describe_table(
    session_manager: &SessionManager,
    vr_store: &VirtualRelationStore,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    connection_id: Option<&str>,
) -> Result<TableSchema, ServiceError> {
    let driver = session_manager.get_driver(session).await?;
    let mut schema = driver.describe_table(session, namespace, table).await?;

    if let Some(conn_id) = connection_id {
        let virtual_fks = vr_store.get_foreign_keys_for_table(
            conn_id,
            &namespace.database,
            namespace.schema.as_deref(),
            table,
        );
        for vfk in virtual_fks {
            let is_duplicate = schema.foreign_keys.iter().any(|fk| {
                fk.column == vfk.column
                    && fk.referenced_table == vfk.referenced_table
                    && fk.referenced_column == vfk.referenced_column
            });
            if !is_duplicate {
                schema.foreign_keys.push(vfk);
            }
        }
    }

    Ok(schema)
}

pub async fn preview_table(
    session_manager: &SessionManager,
    query_manager: &QueryManager,
    query_cache: &QueryCache,
    policy: &SafetyPolicy,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    limit: u32,
    bypass_cache: bool,
) -> Result<QueryResult, ServiceError> {
    let effective_limit = governance::clamp_rows(policy, limit);

    let connection_key = session_manager.connection_key(session).await;
    let use_cache = !bypass_cache && connection_key.is_some();
    let cache_key = format!(
        "preview\u{1}{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}",
        connection_key.as_deref().unwrap_or(""),
        namespace.database,
        namespace.schema.as_deref().unwrap_or(""),
        table,
        effective_limit
    );
    if use_cache {
        if let Some(hit) = query_cache.get(&cache_key) {
            if let Ok(result) = serde_json::from_str::<QueryResult>(&hit.value) {
                return Ok(result);
            }
        }
    }

    governance::check_concurrent_limit(policy, query_manager)
        .await
        .map_err(ServiceError::Message)?;

    let driver = session_manager.get_driver(session).await?;

    match governance::with_timeout(
        policy,
        driver.preview_table(session, namespace, table, effective_limit),
    )
    .await
    {
        Ok(Ok(result)) => {
            if use_cache {
                if let Ok(json) = serde_json::to_string(&result) {
                    query_cache.put(cache_key, connection_key.unwrap_or_default(), json);
                }
            }
            Ok(result)
        }
        Ok(Err(e)) => Err(ServiceError::Engine(e)),
        Err(timeout_msg) => Err(ServiceError::Message(timeout_msg)),
    }
}

pub async fn query_table(
    session_manager: &SessionManager,
    query_manager: &QueryManager,
    query_cache: &QueryCache,
    policy: &SafetyPolicy,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    mut options: TableQueryOptions,
    bypass_cache: bool,
) -> Result<(PaginatedQueryResult, Option<u64>), ServiceError> {
    if let Some(max_rows) = policy.max_result_rows {
        let max_page = max_rows as u32;
        options.page_size = Some(options.page_size.unwrap_or(50).min(max_page));
    }

    let connection_key = session_manager.connection_key(session).await;
    let use_cache = !bypass_cache && connection_key.is_some();
    let cache_key = format!(
        "query\u{1}{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}",
        connection_key.as_deref().unwrap_or(""),
        namespace.database,
        namespace.schema.as_deref().unwrap_or(""),
        table,
        serde_json::to_string(&options).unwrap_or_default()
    );
    if use_cache {
        if let Some(hit) = query_cache.get(&cache_key) {
            if let Ok(result) = serde_json::from_str::<PaginatedQueryResult>(&hit.value) {
                return Ok((result, Some(hit.age_ms)));
            }
        }
    }

    governance::check_concurrent_limit(policy, query_manager)
        .await
        .map_err(ServiceError::Message)?;

    let driver = session_manager.get_driver(session).await?;

    match governance::with_timeout(policy, driver.query_table(session, namespace, table, options))
        .await
    {
        Ok(Ok(result)) => {
            if use_cache {
                if let Ok(json) = serde_json::to_string(&result) {
                    query_cache.put(cache_key, connection_key.unwrap_or_default(), json);
                }
            }
            Ok((result, None))
        }
        Ok(Err(e)) => Err(ServiceError::Engine(e)),
        Err(timeout_msg) => Err(ServiceError::Message(timeout_msg)),
    }
}

pub struct Preflight {
    pub driver: Arc<dyn DataEngine>,
    pub context: QueryContext,
    pub environment: Environment,
    pub read_only: bool,
    pub is_mutation: bool,
    pub is_dangerous: bool,
    pub is_sql_driver: bool,
    pub connection_key: Option<String>,
    pub safety_warning: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub async fn preflight(
    session_manager: &SessionManager,
    query_rate_limiter: &QueryRateLimiter,
    interceptor: &InterceptorPipeline,
    policy: &SafetyPolicy,
    session: SessionId,
    session_id: &str,
    query: &str,
    namespace: Option<&Namespace>,
    acknowledged: bool,
) -> Result<Preflight, String> {
    let connection_key = session_manager.connection_key(session).await;

    if policy.query_rate_limit_enabled && !query_rate_limiter.try_acquire(session_id) {
        return Err(RATE_LIMIT_BLOCKED.to_string());
    }

    let read_only = session_manager
        .is_read_only(session)
        .await
        .map_err(|e| e.sanitized_message())?;

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;

    let environment_str = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let environment = map_environment(&environment_str);
    let is_production = matches!(environment, Environment::Production);

    let is_mongo_driver = driver.driver_id().eq_ignore_ascii_case("mongodb");
    let is_redis_driver = driver.driver_id().eq_ignore_ascii_case("redis");
    let is_sql_driver = !is_mongo_driver && !is_redis_driver;

    let sql_analysis = if is_sql_driver {
        match sql_safety::analyze_sql(driver.driver_id(), query) {
            Ok(analysis) => Some(analysis),
            Err(err) => {
                if read_only {
                    return Err(format!("{SQL_PARSE_BLOCKED}: {err}"));
                }
                if is_production {
                    if policy.prod_block_dangerous_sql {
                        return Err(format!("{DANGEROUS_BLOCKED_POLICY}: SQL parse error: {err}"));
                    }
                    if policy.prod_require_confirmation && !acknowledged {
                        return Err(format!("{DANGEROUS_BLOCKED}: SQL parse error: {err}"));
                    }
                }
                None
            }
        }
    } else {
        None
    };

    if read_only {
        let is_mutation = if is_sql_driver {
            sql_analysis.as_ref().map(|a| a.is_mutation).unwrap_or(false)
        } else if is_mongo_driver {
            is_mongo_mutation(query)
        } else {
            is_redis_mutation(query)
        };
        if is_mutation {
            return Err(READ_ONLY_BLOCKED.to_string());
        }
    }

    if is_production {
        let is_dangerous = if is_sql_driver {
            sql_analysis
                .as_ref()
                .map(|a| a.is_dangerous)
                .unwrap_or(false)
        } else if is_redis_driver {
            is_redis_dangerous(query)
        } else {
            false
        };
        if is_dangerous {
            if policy.prod_block_dangerous_sql {
                return Err(DANGEROUS_BLOCKED_POLICY.to_string());
            }
            if policy.prod_require_confirmation && !acknowledged {
                return Err(DANGEROUS_BLOCKED.to_string());
            }
        }
    }

    let is_mutation = if is_sql_driver {
        sql_analysis.as_ref().map(|a| a.is_mutation).unwrap_or(false)
    } else if is_mongo_driver {
        is_mongo_mutation(query)
    } else {
        is_redis_mutation(query)
    };

    let is_dangerous = sql_analysis
        .as_ref()
        .map(|a| a.is_dangerous)
        .unwrap_or(false);

    let context = interceptor.build_context(
        session_id,
        query,
        driver.driver_id(),
        environment,
        read_only,
        acknowledged,
        namespace.map(|n| n.database.as_str()),
        sql_analysis.as_ref(),
        is_mutation,
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

    Ok(Preflight {
        driver,
        context,
        environment,
        read_only,
        is_mutation,
        is_dangerous,
        is_sql_driver,
        connection_key,
        safety_warning,
    })
}
