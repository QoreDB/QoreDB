// SPDX-License-Identifier: BUSL-1.1

//! Data Generator (Pro)
//!
//! Generates realistic seed data for a table while respecting its schema:
//! column types, nullability, foreign keys (values are drawn from existing
//! rows of the referenced table) and DB-managed columns (those with a default
//! are skipped so the database fills them). Output is a multi-row `INSERT`
//! script the caller can preview, export, or execute.

use std::collections::HashMap;
use std::sync::Arc;

use rand::seq::SliceRandom;
use rand::Rng;
use tauri::State;
use uuid::Uuid;

use qore_sql::generator::SqlDialect;

use crate::engine::types::{Namespace, SessionId, TableColumn, TableQueryOptions, Value};

/// Rows per `INSERT` statement. SQLite caps a multi-row VALUES list at 500, so
/// we chunk to stay portable across the supported dialects.
const CHUNK_SIZE: usize = 500;
/// Hard ceiling on generated rows per call.
const MAX_ROWS: u32 = 10_000;
/// How many existing values to sample per foreign-key column.
const FK_SAMPLE_SIZE: u32 = 500;

const FIRST_NAMES: &[&str] = &[
    "Alex", "Marie", "Liam", "Sofia", "Noah", "Emma", "Lucas", "Olivia", "Hugo", "Lina", "Adam",
    "Chloe", "Gabriel", "Jade", "Louis", "Alice",
];
const LAST_NAMES: &[&str] = &[
    "Martin", "Bernard", "Dubois", "Petit", "Garcia", "Roux", "Moreau", "Lopez", "Fontaine",
    "Lambert", "Rossi", "Costa", "Silva", "Muller",
];
const CITIES: &[&str] = &[
    "Paris", "Lyon", "Berlin", "Madrid", "Lisboa", "Roma", "Tokyo", "Seoul", "Austin", "Toronto",
];
const COUNTRIES: &[&str] = &[
    "France", "Germany", "Spain", "Portugal", "Italy", "Japan", "Canada",
];
const WORDS: &[&str] = &[
    "alpha", "bravo", "lorem", "ipsum", "delta", "omega", "nova", "vertex", "quartz", "ember",
];

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedDataResult {
    /// Generated `INSERT` script (one or more statements separated by `;\n`).
    pub sql: String,
    /// Number of rows actually generated.
    pub row_count: usize,
    /// Non-fatal notices (e.g. a foreign-key target with no rows to draw from).
    pub warnings: Vec<String>,
}

fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(SessionId(uuid))
}

#[cfg(feature = "pro")]
async fn license_allows_pro(state: &State<'_, crate::SharedState>) -> bool {
    let tier = {
        let guard = state.lock().await;
        guard.license_manager.effective_status().tier
    };
    tier.includes(crate::license::status::LicenseTier::Pro)
}

#[cfg(not(feature = "pro"))]
async fn license_allows_pro(_state: &State<'_, crate::SharedState>) -> bool {
    false
}

/// Generates a seed `INSERT` script for `table`. Does not touch the database —
/// the caller previews, exports, or executes the returned SQL.
#[tauri::command]
pub async fn generate_seed_data(
    state: State<'_, crate::SharedState>,
    session_id: String,
    namespace: Namespace,
    table: String,
    count: u32,
    connection_id: Option<String>,
) -> Result<SeedDataResult, String> {
    if !license_allows_pro(&state).await {
        return Err("The Data Generator requires a Pro license.".to_string());
    }

    let count = count.clamp(1, MAX_ROWS);

    let (session_manager, vr_store, query_manager, query_cache, policy) = {
        let s = state.lock().await;
        (
            Arc::clone(&s.session_manager),
            Arc::clone(&s.virtual_relations),
            Arc::clone(&s.query_manager),
            Arc::clone(&s.query_cache),
            s.policy.clone(),
        )
    };

    let session = parse_session_id(&session_id)?;

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;
    let dialect = SqlDialect::from_driver_id(driver.driver_id()).ok_or_else(|| {
        "The Data Generator only supports SQL databases (Postgres, MySQL/MariaDB, SQLite, SQL Server)."
            .to_string()
    })?;

    let schema = qore_service::query::describe_table(
        &session_manager,
        &vr_store,
        session,
        &namespace,
        &table,
        connection_id.as_deref(),
    )
    .await
    .map_err(|e| e.sanitized())?;

    // Columns the DB does not fill itself: skip those with a default
    // (serial, now()…) and auto-increment / IDENTITY / rowid columns, which the
    // database assigns and which may reject an explicit value.
    let target_columns: Vec<TableColumn> = schema
        .columns
        .iter()
        .filter(|c| c.default_value.is_none() && !c.is_auto_increment)
        .cloned()
        .collect();

    if target_columns.is_empty() {
        return Err("Every column has a default value — nothing to generate.".to_string());
    }

    let mut warnings: Vec<String> = Vec::new();

    // Columns constrained by a foreign key. These must never fall back to a
    // generated value: an invented key would violate the constraint.
    let fk_columns: std::collections::HashSet<&str> = schema
        .foreign_keys
        .iter()
        .map(|fk| fk.column.as_str())
        .collect();

    // Pre-fetch existing values for each foreign-key column so generated rows
    // reference real parents (drawn from a sample of the referenced table).
    let mut fk_values: HashMap<String, Vec<Value>> = HashMap::new();
    for col in &target_columns {
        let Some(fk) = schema.foreign_keys.iter().find(|fk| fk.column == col.name) else {
            continue;
        };

        let target_ns = Namespace {
            database: fk
                .referenced_database
                .clone()
                .unwrap_or_else(|| namespace.database.clone()),
            schema: fk
                .referenced_schema
                .clone()
                .or_else(|| namespace.schema.clone()),
        };
        let options = TableQueryOptions {
            page_size: Some(FK_SAMPLE_SIZE),
            ..Default::default()
        };

        match qore_service::query::query_table(
            &session_manager,
            &query_manager,
            &query_cache,
            &policy,
            session,
            &target_ns,
            &fk.referenced_table,
            options,
            false,
        )
        .await
        {
            Ok((paginated, _)) => {
                let idx = paginated
                    .result
                    .columns
                    .iter()
                    .position(|c| c.name == fk.referenced_column);
                let vals: Vec<Value> = match idx {
                    Some(idx) => paginated
                        .result
                        .rows
                        .into_iter()
                        .filter_map(|r| r.values.into_iter().nth(idx))
                        .filter(|v| !matches!(v, Value::Null))
                        .collect(),
                    None => Vec::new(),
                };
                if vals.is_empty() {
                    if col.nullable {
                        warnings.push(format!(
                            "Foreign key '{}' references '{}' which has no rows; NULL will be used.",
                            col.name, fk.referenced_table
                        ));
                    } else {
                        warnings.push(format!(
                            "Foreign key '{}' references '{}' which has no rows and is NOT NULL; no valid value can be generated for this column.",
                            col.name, fk.referenced_table
                        ));
                    }
                } else {
                    fk_values.insert(col.name.clone(), vals);
                }
            }
            Err(e) => warnings.push(format!(
                "Could not sample foreign key '{}': {}",
                col.name,
                e.sanitized()
            )),
        }
    }

    // Synchronous generation block (rng is not Send across await points).
    let sql = {
        let mut rng = rand::thread_rng();
        let mut rows: Vec<Vec<Value>> = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            let mut row: Vec<Value> = Vec::with_capacity(target_columns.len());
            for col in &target_columns {
                let value = if let Some(candidates) = fk_values.get(&col.name) {
                    candidates.choose(&mut rng).cloned().unwrap_or(Value::Null)
                } else if fk_columns.contains(col.name.as_str()) {
                    // Foreign key with no parent rows to draw from: NULL is the
                    // only value that cannot violate the constraint.
                    Value::Null
                } else {
                    generate_value(col, i, &mut rng)
                };
                row.push(value);
            }
            rows.push(row);
        }
        build_insert_sql(&dialect, &namespace, &table, &target_columns, &rows)
    };

    Ok(SeedDataResult {
        sql,
        row_count: count as usize,
        warnings,
    })
}

fn generate_value(col: &TableColumn, index: usize, rng: &mut impl Rng) -> Value {
    let name = col.name.to_lowercase();
    let dt = col.data_type.to_lowercase();

    let is_textual = dt.contains("char")
        || dt.contains("text")
        || dt.contains("string")
        || dt.contains("clob")
        || dt.contains("uuid");
    if is_textual {
        if name.contains("email") {
            return Value::Text(format!("user{}@example.com", index + 1));
        }
        if name.contains("first") && name.contains("name") {
            return Value::Text(pick(FIRST_NAMES, rng));
        }
        if (name.contains("last") || name.contains("sur")) && name.contains("name") {
            return Value::Text(pick(LAST_NAMES, rng));
        }
        if name.contains("name") {
            return Value::Text(format!(
                "{} {}",
                pick(FIRST_NAMES, rng),
                pick(LAST_NAMES, rng)
            ));
        }
        if name.contains("phone") {
            return Value::Text(format!("+1{:010}", rng.gen_range(0u64..9_999_999_999)));
        }
        if name.contains("city") {
            return Value::Text(pick(CITIES, rng));
        }
        if name.contains("country") {
            return Value::Text(pick(COUNTRIES, rng));
        }
    }

    if dt.contains("uuid") {
        return Value::Text(Uuid::new_v4().to_string());
    }
    if dt.contains("bool") {
        return Value::Bool(rng.gen());
    }
    if dt.contains("json") {
        return Value::Json(serde_json::json!({ "n": index + 1 }));
    }
    if dt.contains("timestamp") || dt.contains("datetime") {
        return Value::Text(format!("{} 12:00:00", random_date(rng)));
    }
    if dt.contains("date") {
        return Value::Text(random_date(rng));
    }
    if dt.contains("time") {
        return Value::Text(format!(
            "{:02}:{:02}:00",
            rng.gen_range(0..24),
            rng.gen_range(0..60)
        ));
    }
    if dt.contains("float")
        || dt.contains("double")
        || dt.contains("real")
        || dt.contains("numeric")
        || dt.contains("decimal")
    {
        let v = (rng.gen_range(0.0f64..10_000.0) * 100.0).round() / 100.0;
        return Value::Float(v);
    }
    if dt.contains("int") || dt.contains("serial") {
        return Value::Int(rng.gen_range(1..1_000_000));
    }

    Value::Text(format!("{}_{}", pick(WORDS, rng), index + 1))
}

fn pick(list: &[&str], rng: &mut impl Rng) -> String {
    (*list.choose(rng).unwrap_or(&list[0])).to_string()
}

fn random_date(rng: &mut impl Rng) -> String {
    use chrono::{Duration, NaiveDate};
    let base = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
    let date = base + Duration::days(rng.gen_range(0..2190));
    date.format("%Y-%m-%d").to_string()
}

fn build_insert_sql(
    dialect: &SqlDialect,
    namespace: &Namespace,
    table: &str,
    columns: &[TableColumn],
    rows: &[Vec<Value>],
) -> String {
    let qualified = dialect.qualified_table(namespace, table);
    let cols = columns
        .iter()
        .map(|c| dialect.quote_ident(&c.name))
        .collect::<Vec<_>>()
        .join(", ");

    let mut statements: Vec<String> = Vec::new();
    for chunk in rows.chunks(CHUNK_SIZE) {
        let values = chunk
            .iter()
            .map(|row| {
                let tuple = row
                    .iter()
                    .map(|v| dialect.format_value(v))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({})", tuple)
            })
            .collect::<Vec<_>>()
            .join(",\n  ");
        statements.push(format!(
            "INSERT INTO {} ({})\nVALUES\n  {};",
            qualified, cols, values
        ));
    }
    statements.join("\n\n")
}
