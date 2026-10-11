// SPDX-License-Identifier: Apache-2.0

//! Optional row reads isolated from the caller's transaction and cancellation.

use std::time::Duration;

use super::*;
use qore_core::{CountMode, OrderingGuarantee, PaginationStrategy};
use sqlx::PgConnection;
use tokio::time::timeout;

const READ_TIMEOUT_MS: u64 = 1500;
const RECOVERY_TIMEOUT: Duration = Duration::from_secs(4);

pub(crate) async fn query_table(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    options: TableQueryOptions,
) -> EngineResult<PaginatedQueryResult> {
    // This entry point intentionally accepts only the bounded identity lookup
    // used for mutation evidence; it never counts or sorts.
    if options.effective_page() != 1
        || options.effective_page_size() != 2
        || !matches!(options.effective_count_mode(), CountMode::None)
        || options.effective_search().is_some()
        || options.sort_column.is_some()
        || options.cursor.is_some()
    {
        return Err(EngineError::not_supported(
            "Unsupported capture query options",
        ));
    }
    let filters = options.filters.unwrap_or_default();
    if filters.is_empty()
        || filters
            .iter()
            .any(|filter| !matches!(filter.operator, FilterOperator::Eq))
    {
        return Err(EngineError::validation("Capture requires equality filters"));
    }
    let pg = get_session(sessions, session).await?;
    let table = qualified_table_name(namespace, table, false);
    let predicates = filters
        .iter()
        .enumerate()
        .map(|(index, filter)| format!("{} = ${}", quote_ident(&filter.column), index + 1))
        .collect::<Vec<_>>()
        .join(" AND ");
    let sql = format!("SELECT * FROM {table} WHERE {predicates} LIMIT 2");

    // The service can abandon the read at its deadline. Recovery must still own
    // the connection lock until rollback/release finishes; dropping a join
    // handle leaves this bounded task running.
    tokio::spawn(async move {
        let start = Instant::now();
        let mut transaction = timeout(Duration::from_secs(2), pg.transaction_conn.lock())
            .await
            .map_err(|_| EngineError::Timeout { timeout_ms: 2000 })?;
        pg.ensure_transaction_usable()?;
        let rows = if let Some(conn) = transaction.as_mut() {
            match timeout(
                RECOVERY_TIMEOUT,
                read_isolated(conn, true, &table, &sql, &filters),
            )
            .await
            {
                Ok(Ok(rows)) => rows,
                failure => {
                    // An unrecovered connection must never be reused for a
                    // write or reported as committed while its state is unknown.
                    pg.transaction_failed.store(true, Ordering::Release);
                    if let Some(mut conn) = transaction.take() {
                        conn.close_on_drop();
                    }
                    return Err(recovery_error(failure));
                }
            }
        } else {
            let mut conn = timeout(Duration::from_secs(2), pg.pool.acquire())
                .await
                .map_err(|_| EngineError::Timeout { timeout_ms: 2000 })?
                .map_err(|error| EngineError::connection_failed(error.to_string()))?;
            match timeout(
                RECOVERY_TIMEOUT,
                read_isolated(&mut conn, false, &table, &sql, &filters),
            )
            .await
            {
                Ok(Ok(rows)) => rows,
                failure => {
                    conn.close_on_drop();
                    return Err(recovery_error(failure));
                }
            }
        };
        drop(transaction);
        let rows = rows?;
        let result = timeout(
            Duration::from_secs(2),
            rows_to_result(rows, &pg.pool, start),
        )
        .await
        .map_err(|_| EngineError::Timeout { timeout_ms: 2000 })??;
        Ok(PaginatedQueryResult {
            result,
            page: 1,
            page_size: 2,
            total_rows: None,
            total_rows_source: None,
            total_rows_as_of: None,
            has_more: false,
            next_cursor: None,
            pagination_strategy: PaginationStrategy::Offset,
            ordering_guarantee: OrderingGuarantee::None,
        })
    })
    .await
    .map_err(|error| EngineError::execution_error(format!("Capture read task failed: {error}")))?
}

fn recovery_error<T>(result: Result<EngineResult<T>, tokio::time::error::Elapsed>) -> EngineError {
    match result {
        Ok(Err(error)) => error,
        _ => EngineError::Timeout {
            timeout_ms: RECOVERY_TIMEOUT.as_millis() as u64,
        },
    }
}

// Outer errors mean cleanup failed; inner errors mean the read failed but the
// connection is restored and the caller's preceding writes are still intact.
async fn read_isolated(
    conn: &mut PgConnection,
    in_transaction: bool,
    table: &str,
    sql: &str,
    filters: &[qore_core::ColumnFilter],
) -> EngineResult<EngineResult<Vec<PgRow>>> {
    let savepoint = format!("qoredb_capture_{}", uuid::Uuid::new_v4().simple());
    let begin = if in_transaction {
        format!("SAVEPOINT {savepoint}")
    } else {
        "BEGIN READ ONLY".into()
    };
    sqlx::query(&begin)
        .execute(&mut *conn)
        .await
        .map_err(|error| EngineError::transaction_error(error.to_string()))?;
    let rows = async {
        // Keep a stricter caller timeout. ROLLBACK TO restores the prior local
        // setting on both success and failure, including after cancellation.
        sqlx::query("SELECT set_config('statement_timeout', LEAST(NULLIF(setting::bigint, 0), $1)::text, true) FROM pg_settings WHERE name = 'statement_timeout'")
            .bind(READ_TIMEOUT_MS as i64).execute(&mut *conn).await?;
        let unrestricted = sqlx::query_scalar::<_, bool>("SELECT NOT row_security_active($1::text::regclass)")
            .bind(table).fetch_one(&mut *conn).await?;
        if !unrestricted {
            return Ok(None);
        }
        let mut query = sqlx::query(sql);
        for filter in filters {
            query = bind_param(query, &filter.value);
        }
        query.fetch_all(&mut *conn).await.map(Some)
    }.await.map_err(|error: sqlx::Error| EngineError::execution_error(error.to_string()))
        .and_then(|rows| rows.ok_or_else(|| EngineError::not_supported("Row security prevents complete capture")));

    // Always discard side effects of optional reads as well as local settings.
    let rollback = if in_transaction {
        format!("ROLLBACK TO SAVEPOINT {savepoint}")
    } else {
        "ROLLBACK".into()
    };
    sqlx::query(&rollback)
        .execute(&mut *conn)
        .await
        .map_err(|error| EngineError::transaction_error(error.to_string()))?;
    if in_transaction {
        sqlx::query(&format!("RELEASE SAVEPOINT {savepoint}"))
            .execute(&mut *conn)
            .await
            .map_err(|error| EngineError::transaction_error(error.to_string()))?;
    }
    Ok(rows)
}
