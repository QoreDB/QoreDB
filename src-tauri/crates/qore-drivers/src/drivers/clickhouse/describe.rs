// SPDX-License-Identifier: Apache-2.0

//! Schema introspection for ClickHouse: databases, tables, and columns.
//!
//! All metadata lives in the `system.*` virtual schema. We avoid
//! `INFORMATION_SCHEMA` because its output drifts between server versions
//! while `system.tables` / `system.columns` have been stable for years and
//! expose ClickHouse-specific knobs (`engine`, `partition_key`, `sorting_key`,
//! `total_rows`) that the UI surfaces.

use qore_core::error::{EngineError, EngineResult};
use qore_core::types::{
    Collection, CollectionList, CollectionListOptions, CollectionType, Namespace, TableColumn,
    TableIndex, TableSchema, Value,
};
use uuid::Uuid;

use super::client::ClickHouseClient;
use super::response::parse_query_result;

pub async fn list_databases(client: &ClickHouseClient) -> EngineResult<Vec<Namespace>> {
    let body = client
        .fetch_json(
            "SELECT name FROM system.databases \
             WHERE name NOT IN ('system','INFORMATION_SCHEMA','information_schema') \
             ORDER BY name",
            None,
        )
        .await?;
    let qr = parse_query_result(&body, 0.0)?;
    let mut out = Vec::with_capacity(qr.rows.len());
    for row in qr.rows {
        if let Some(Value::Text(name)) = row.values.into_iter().next() {
            out.push(Namespace::new(name));
        }
    }
    Ok(out)
}

pub async fn list_tables(
    client: &ClickHouseClient,
    namespace: &Namespace,
    options: CollectionListOptions,
) -> EngineResult<CollectionList> {
    // Bind `database` / `pattern` as `{db:String}` / `{pat:String}` to prevent
    // quote-escape injection, and pre-escape LIKE wildcards in the user-supplied
    // `search` so `50%` matches the literal substring (cf. audit B4-C7).
    let database = namespace.database.clone();
    let like_pattern = options
        .search
        .as_ref()
        .map(|s| format!("%{}%", escape_like(s)));

    let (count_sql, count_params): (&str, Vec<(&str, &str)>) = match like_pattern.as_deref() {
        Some(p) => (
            "SELECT count() FROM system.tables \
             WHERE database = {db:String} AND name ILIKE {pat:String}",
            vec![("db", database.as_str()), ("pat", p)],
        ),
        None => (
            "SELECT count() FROM system.tables WHERE database = {db:String}",
            vec![("db", database.as_str())],
        ),
    };
    let total = parse_query_result(
        &client
            .fetch_json_with_params(count_sql, None, &count_params)
            .await?,
        0.0,
    )?
    .rows
    .into_iter()
    .next()
    .and_then(|r| match r.values.into_iter().next() {
        Some(Value::Int(i)) => Some(i as u32),
        _ => None,
    })
    .unwrap_or(0);

    let mut data_sql = match like_pattern.as_deref() {
        Some(_) => String::from(
            "SELECT name, engine FROM system.tables \
             WHERE database = {db:String} AND name ILIKE {pat:String} \
             ORDER BY name",
        ),
        None => String::from(
            "SELECT name, engine FROM system.tables \
             WHERE database = {db:String} \
             ORDER BY name",
        ),
    };
    if let Some(limit) = options.page_size {
        data_sql.push_str(&format!(" LIMIT {}", limit));
        if let Some(page) = options.page {
            let offset = (page.max(1) - 1) * limit;
            data_sql.push_str(&format!(" OFFSET {}", offset));
        }
    }

    let qr = parse_query_result(
        &client
            .fetch_json_with_params(&data_sql, None, &count_params)
            .await?,
        0.0,
    )?;
    let collections = qr
        .rows
        .into_iter()
        .map(|r| {
            let mut iter = r.values.into_iter();
            let name = match iter.next() {
                Some(Value::Text(s)) => s,
                _ => String::new(),
            };
            let engine = match iter.next() {
                Some(Value::Text(s)) => s,
                _ => String::new(),
            };
            let collection_type = if engine.starts_with("View")
                || engine == "View"
                || engine == "LiveView"
            {
                CollectionType::View
            } else if engine == "MaterializedView" {
                CollectionType::MaterializedView
            } else {
                CollectionType::Table
            };
            Collection {
                namespace: namespace.clone(),
                name,
                collection_type,
            }
        })
        .collect();

    Ok(CollectionList {
        collections,
        total_count: total,
    })
}

pub async fn describe_table(
    client: &ClickHouseClient,
    namespace: &Namespace,
    table: &str,
) -> EngineResult<TableSchema> {
    let db = namespace.database.clone();
    let tbl = table.to_string();
    let params: Vec<(&str, &str)> = vec![("db", db.as_str()), ("tbl", tbl.as_str())];

    // Bind database/table as named parameters to avoid quote-escape injection (audit B4-C7).
    let cols_sql = "SELECT name, type, default_expression, is_in_primary_key \
         FROM system.columns \
         WHERE database = {db:String} AND table = {tbl:String} \
         ORDER BY position";
    let cols = parse_query_result(
        &client
            .fetch_json_with_params(cols_sql, None, &params)
            .await?,
        0.0,
    )?;
    if cols.rows.is_empty() {
        return Err(EngineError::execution_error(format!(
            "Table {db}.{tbl} not found"
        )));
    }

    let mut columns: Vec<TableColumn> = Vec::with_capacity(cols.rows.len());
    let mut primary_key_cols = Vec::new();
    for row in cols.rows {
        let mut it = row.values.into_iter();
        let name = match it.next() {
            Some(Value::Text(s)) => s,
            _ => continue,
        };
        let raw_type = match it.next() {
            Some(Value::Text(s)) => s,
            _ => "Unknown".into(),
        };
        let default_expr = match it.next() {
            Some(Value::Text(s)) if !s.is_empty() => Some(s),
            _ => None,
        };
        let is_pk = matches!(it.next(), Some(Value::Int(i)) if i != 0)
            || matches!(it.next(), Some(Value::Bool(true)));
        let nullable = raw_type
            .to_ascii_uppercase()
            .contains("NULLABLE(");
        if is_pk {
            primary_key_cols.push(name.clone());
        }
        columns.push(TableColumn {
            name,
            data_type: raw_type,
            nullable,
            default_value: default_expr,
            is_primary_key: is_pk,
        });
    }

    // Approximate row count via system.tables.total_rows (only populated for
    // engines that track it: MergeTree family, etc.).
    let count_sql = "SELECT total_rows FROM system.tables \
         WHERE database = {db:String} AND name = {tbl:String}";
    let row_count_estimate =
        match parse_query_result(
            &client
                .fetch_json_with_params(count_sql, None, &params)
                .await?,
            0.0,
        ) {
            Ok(qr) => qr
                .rows
                .into_iter()
                .next()
                .and_then(|r| match r.values.into_iter().next() {
                    Some(Value::Int(i)) if i >= 0 => Some(i as u64),
                    _ => None,
                }),
            Err(_) => None,
        };

    let primary_key = if primary_key_cols.is_empty() {
        None
    } else {
        Some(primary_key_cols)
    };

    let indexes = fetch_indexes(client, &db, &tbl).await.unwrap_or_default();

    Ok(TableSchema {
        columns,
        primary_key,
        foreign_keys: Vec::new(), // ClickHouse has no FK enforcement.
        row_count_estimate,
        indexes,
    })
}

/// Reads `system.data_skipping_indices` to surface MergeTree data-skipping
/// indexes (bloom_filter / minmax / set / ngrambf_v1 …). These don't enforce
/// uniqueness — they accelerate WHERE filtering — so `is_unique` and
/// `is_primary` are always false.
async fn fetch_indexes(
    client: &ClickHouseClient,
    db: &str,
    tbl: &str,
) -> EngineResult<Vec<TableIndex>> {
    let sql = "SELECT name, type, expr \
         FROM system.data_skipping_indices \
         WHERE database = {db:String} AND table = {tbl:String} \
         ORDER BY name";
    let params: Vec<(&str, &str)> = vec![("db", db), ("tbl", tbl)];
    let qr = match parse_query_result(
        &client.fetch_json_with_params(sql, None, &params).await?,
        0.0,
    ) {
        Ok(qr) => qr,
        Err(_) => return Ok(Vec::new()),
    };

    let mut out = Vec::with_capacity(qr.rows.len());
    for row in qr.rows {
        let mut it = row.values.into_iter();
        let name = match it.next() {
            Some(Value::Text(s)) => s,
            _ => continue,
        };
        let index_type = match it.next() {
            Some(Value::Text(s)) if !s.is_empty() => Some(s),
            _ => None,
        };
        // The `expr` column carries the comma-separated column list (or a
        // free-form expression). We store the whole expression as a single
        // virtual column entry so the UI can display it verbatim.
        let expr = match it.next() {
            Some(Value::Text(s)) => s,
            _ => String::new(),
        };
        let columns = if expr.is_empty() { Vec::new() } else { vec![expr] };

        out.push(TableIndex {
            name,
            columns,
            is_unique: false,
            is_primary: false,
            index_type,
        });
    }
    Ok(out)
}

/// Escape ClickHouse `LIKE` / `ILIKE` wildcards in user-supplied search
/// terms. Without this, a search for `50%` matches *every* name containing
/// "50" because `%` is the multi-character wildcard.
fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '%' => out.push_str("\\%"),
            '_' => out.push_str("\\_"),
            other => out.push(other),
        }
    }
    out
}

/// Pings the server. We use `SELECT 1` + a fresh query_id so it shows up
/// in `system.query_log` distinctly from real workload.
pub async fn ping(client: &ClickHouseClient) -> EngineResult<()> {
    let qid = Uuid::new_v4();
    let body = client.fetch_json("SELECT 1", Some(&qid)).await?;
    let qr = parse_query_result(&body, 0.0)?;
    if qr
        .rows
        .first()
        .and_then(|r| r.values.first())
        .map(|v| matches!(v, Value::Int(1)))
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(EngineError::execution_error("Ping failed"))
    }
}
