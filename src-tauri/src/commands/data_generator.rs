// SPDX-License-Identifier: BUSL-1.1

//! Data Generator (Pro)
//!
//! Generates realistic seed data for a table while respecting its schema:
//! column types, nullability, foreign keys (values are drawn from existing
//! rows of the referenced table) and DB-managed columns (those with a default
//! are skipped so the database fills them). Output is a multi-row `INSERT`
//! script the caller can preview, export, or execute.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rand::Rng;
use rand::seq::SliceRandom;
use tauri::State;
use uuid::Uuid;

use qore_sql::generator::SqlDialect;

use super::parse_session_id;
use crate::engine::types::{
    ColumnFilter, CountMode, FilterOperator, Namespace, SortDirection, TableColumn,
    TableQueryOptions, TableSchema, Value,
};

/// Rows per `INSERT` statement. SQLite caps a multi-row VALUES list at 500, so
/// we chunk to stay portable across the supported dialects.
const CHUNK_SIZE: usize = 500;
/// Hard ceiling on generated rows per call.
const MAX_ROWS: u32 = 10_000;
/// How many existing values to sample per foreign-key column.
const FK_SAMPLE_SIZE: u32 = 500;
/// Attempts to draw a row whose composite-key tuple is not already used.
const COMPOSITE_RETRIES: usize = 64;

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
    /// Structured row values keyed by column name, used to stage generated rows in Sandbox mode.
    pub rows: Vec<HashMap<String, Value>>,
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
            count_mode: Some(CountMode::None),
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
                        return Err(format!(
                            "Foreign key column '{}' is NOT NULL, but referenced table '{}' has no rows to sample from.",
                            col.name, fk.referenced_table
                        ));
                    }
                } else {
                    fk_values.insert(col.name.clone(), vals);
                }
            }
            Err(e) => {
                if !col.nullable {
                    return Err(format!(
                        "Could not sample NOT NULL foreign key '{}': {}",
                        col.name,
                        e.sanitized()
                    ));
                }
                warnings.push(format!(
                    "Could not sample foreign key '{}': {}",
                    col.name,
                    e.sanitized()
                ));
            }
        }
    }

    let existing = {
        let fetch = |options: TableQueryOptions| {
            qore_service::query::query_table(
                &session_manager,
                &query_manager,
                &query_cache,
                &policy,
                session,
                &namespace,
                &table,
                options,
                false,
            )
        };
        let row_count = match fetch(TableQueryOptions {
            page_size: Some(1),
            count_mode: Some(CountMode::Exact),
            ..Default::default()
        })
        .await
        {
            Ok((paginated, _)) => paginated.total_rows.unwrap_or(0) as usize,
            Err(e) => {
                warnings.push(format!(
                    "Could not count existing rows; generated unique values may collide: {}",
                    e.sanitized()
                ));
                0
            }
        };
        let mut int_max = HashMap::new();
        if row_count > 0 {
            let unique = unique_column_names(&schema);
            let fk_cols: HashSet<&str> = schema
                .foreign_keys
                .iter()
                .map(|fk| fk.column.as_str())
                .collect();
            for col in target_columns.iter().filter(|c| {
                unique.contains(c.name.as_str())
                    && !fk_cols.contains(c.name.as_str())
                    && int_bounds(&c.data_type.to_lowercase()).is_some()
            }) {
                let options = TableQueryOptions {
                    page_size: Some(1),
                    count_mode: Some(CountMode::None),
                    sort_column: Some(col.name.clone()),
                    sort_direction: Some(SortDirection::Desc),
                    filters: Some(vec![ColumnFilter {
                        column: col.name.clone(),
                        operator: FilterOperator::IsNotNull,
                        value: Value::Null,
                        options: Default::default(),
                    }]),
                    ..Default::default()
                };
                match fetch(options).await {
                    Ok((paginated, _)) => {
                        let max = paginated
                            .result
                            .columns
                            .iter()
                            .position(|c| c.name == col.name)
                            .and_then(|idx| paginated.result.rows.first()?.values.get(idx).cloned());
                        if let Some(Value::Int(max)) = max {
                            int_max.insert(col.name.clone(), max);
                        }
                    }
                    Err(e) => warnings.push(format!(
                        "Could not read the current maximum of '{}'; generated values may collide: {}",
                        col.name,
                        e.sanitized()
                    )),
                }
            }
        }
        ExistingRows { row_count, int_max }
    };

    // Synchronous generation block (rng is not Send across await points).
    let (sql, row_maps) = {
        let rows = generate_seed_rows(
            &schema,
            &target_columns,
            &fk_values,
            &existing,
            count as usize,
        )?;
        let row_maps: Vec<HashMap<String, Value>> = rows
            .iter()
            .map(|row| {
                target_columns
                    .iter()
                    .zip(row.iter())
                    .map(|(col, val)| (col.name.clone(), val.clone()))
                    .collect()
            })
            .collect();
        (
            build_insert_sql(&dialect, &namespace, &table, &target_columns, &rows),
            row_maps,
        )
    };

    Ok(SeedDataResult {
        sql,
        row_count: count as usize,
        warnings,
        rows: row_maps,
    })
}

/// What already lives in the target table, so generated unique values start
/// past it instead of colliding with a previous seed run.
#[derive(Debug, Default)]
struct ExistingRows {
    row_count: usize,
    int_max: HashMap<String, i64>,
}

fn primary_key_columns(schema: &TableSchema) -> Vec<&str> {
    match &schema.primary_key {
        Some(cols) if !cols.is_empty() => cols.iter().map(String::as_str).collect(),
        _ => schema
            .columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.as_str())
            .collect(),
    }
}

/// Columns that must be unique on their own. A column that is only part of a
/// composite key may repeat; see [`composite_unique_keys`].
fn unique_column_names(schema: &TableSchema) -> HashSet<&str> {
    let mut unique = HashSet::new();
    if let [single] = primary_key_columns(schema).as_slice() {
        unique.insert(*single);
    }
    for idx in &schema.indexes {
        if idx.is_unique && idx.columns.len() == 1 {
            unique.insert(idx.columns[0].as_str());
        }
    }
    unique
}

/// Multi-column unique constraints as indexes into `target_columns`. A key
/// with a database-filled column is skipped: its tuple cannot be known here.
fn composite_unique_keys(schema: &TableSchema, target_columns: &[TableColumn]) -> Vec<Vec<usize>> {
    let position = |name: &str| target_columns.iter().position(|c| c.name == name);
    let mut keys: Vec<Vec<usize>> = Vec::new();
    let pk = primary_key_columns(schema);
    let candidates = std::iter::once(pk).filter(|cols| cols.len() > 1).chain(
        schema
            .indexes
            .iter()
            .filter(|idx| idx.is_unique && idx.columns.len() > 1)
            .map(|idx| idx.columns.iter().map(String::as_str).collect()),
    );
    for cols in candidates {
        let Some(mut key) = cols
            .into_iter()
            .map(position)
            .collect::<Option<Vec<usize>>>()
        else {
            continue;
        };
        key.sort_unstable();
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

fn generate_seed_rows(
    schema: &TableSchema,
    target_columns: &[TableColumn],
    fk_values: &HashMap<String, Vec<Value>>,
    existing: &ExistingRows,
    count: usize,
) -> Result<Vec<Vec<Value>>, String> {
    let fk_map: HashMap<&str, &str> = schema
        .foreign_keys
        .iter()
        .map(|fk| (fk.column.as_str(), fk.referenced_table.as_str()))
        .collect();
    let unique_cols = unique_column_names(schema);
    let composite_keys = composite_unique_keys(schema, target_columns);
    let mut seen_composite: Vec<HashSet<String>> = vec![HashSet::new(); composite_keys.len()];
    let int_start = |col: &TableColumn| {
        existing
            .int_max
            .get(&col.name)
            .map_or(1, |max| max.saturating_add(1).max(1))
    };

    let mut rng = rand::thread_rng();
    let mut rows: Vec<Vec<Value>> = Vec::with_capacity(count);
    let mut seen_unique: HashMap<usize, HashSet<String>> = HashMap::new();
    let mut shuffled_fk_pools: HashMap<usize, Vec<Value>> = HashMap::new();

    for (col_idx, col) in target_columns.iter().enumerate() {
        let is_unique = unique_cols.contains(col.name.as_str());
        if let Some(ref_table) = fk_map.get(col.name.as_str()) {
            match fk_values.get(&col.name) {
                Some(candidates) if !candidates.is_empty() => {
                    if is_unique {
                        let mut pool = candidates.clone();
                        pool.shuffle(&mut rng);
                        if pool.len() < count {
                            return Err(format!(
                                "Unique foreign key column '{}' needs {} distinct parent values, but '{}' only has {}.",
                                col.name,
                                count,
                                ref_table,
                                pool.len()
                            ));
                        }
                        shuffled_fk_pools.insert(col_idx, pool);
                    }
                }
                _ => {
                    if !col.nullable {
                        return Err(format!(
                            "Foreign key column '{}' is NOT NULL, but referenced table '{}' has no rows to sample from.",
                            col.name, ref_table
                        ));
                    }
                }
            }
        } else if is_unique
            && let Some((_, max_int)) = int_bounds(&col.data_type.to_lowercase())
            && int_start(col).saturating_add(count as i64 - 1) > max_int
        {
            return Err(format!(
                "Cannot generate {} unique values for '{}' ({}): maximum is {}.",
                count, col.name, col.data_type, max_int
            ));
        }
    }

    for i in 0..count {
        let ordinal = existing.row_count + i;
        let mut attempt = 0;
        let row = loop {
            let mut row: Vec<Value> = Vec::with_capacity(target_columns.len());
            for (col_idx, col) in target_columns.iter().enumerate() {
                let is_unique = unique_cols.contains(col.name.as_str());
                let value = if let Some(pool) = shuffled_fk_pools.get(&col_idx) {
                    pool[i].clone()
                } else if let Some(candidates) = fk_values.get(&col.name) {
                    candidates.choose(&mut rng).cloned().unwrap_or(Value::Null)
                } else if fk_map.contains_key(col.name.as_str()) {
                    Value::Null
                } else if is_unique && int_bounds(&col.data_type.to_lowercase()).is_some() {
                    Value::Int(int_start(col) + i as i64)
                } else {
                    generate_column_value(
                        col,
                        ordinal,
                        is_unique,
                        &mut seen_unique,
                        col_idx,
                        &mut rng,
                    )?
                };
                row.push(value);
            }
            let tuples: Vec<String> = composite_keys
                .iter()
                .map(|key| {
                    key.iter()
                        .map(|&idx| value_dedup_key(&row[idx]))
                        .collect::<Vec<_>>()
                        .join("\u{1f}")
                })
                .collect();
            let fresh = tuples
                .iter()
                .zip(&seen_composite)
                .all(|(tuple, seen)| !seen.contains(tuple));
            if fresh {
                for (tuple, seen) in tuples.into_iter().zip(seen_composite.iter_mut()) {
                    seen.insert(tuple);
                }
                break row;
            }
            attempt += 1;
            if attempt >= COMPOSITE_RETRIES {
                return Err(format!(
                    "Cannot generate {count} rows with distinct composite keys: the available value combinations are exhausted."
                ));
            }
        };
        rows.push(row);
    }

    Ok(rows)
}

fn generate_column_value(
    col: &TableColumn,
    index: usize,
    is_unique: bool,
    seen_unique: &mut HashMap<usize, HashSet<String>>,
    col_idx: usize,
    rng: &mut impl Rng,
) -> Result<Value, String> {
    let raw = generate_value(col, index, is_unique, rng);
    if !is_unique {
        return Ok(raw);
    }

    let seen = seen_unique.entry(col_idx).or_default();
    let key = value_dedup_key(&raw);
    if seen.insert(key) {
        return Ok(raw);
    }

    // Fall back to a deterministic unique string constrained to the column length.
    let max_len = parse_max_char_length(&col.data_type);
    let fallback = format_unique_string("u", index + 1, max_len);
    let fallback_key = fallback.clone();
    if !seen.insert(fallback_key) {
        return Err(format!(
            "Cannot generate enough unique values for column '{}' ({}).",
            col.name, col.data_type
        ));
    }
    Ok(Value::Text(fallback))
}

fn value_dedup_key(val: &Value) -> String {
    match val {
        Value::Null => "<null>".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Text(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Extracts `N` from character types like `varchar(10)`, `char(2)`, `character varying(32)`.
fn parse_max_char_length(data_type: &str) -> Option<usize> {
    let dt = data_type.to_lowercase();
    if !(dt.contains("char") || dt.contains("character")) {
        return None;
    }
    let open = dt.find('(')?;
    let rest = &dt[open + 1..];
    let close = rest.find(')')?;
    let inside = rest[..close].trim();
    let digits: String = inside.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<usize>().ok()
}

fn int_bounds(dt: &str) -> Option<(i64, i64)> {
    if dt.contains("tinyint") || dt == "int1" {
        return Some((1, 127));
    }
    if dt.contains("smallint") || dt.contains("smallserial") || dt == "int2" {
        return Some((1, 32_767));
    }
    if dt.contains("int") || dt.contains("serial") {
        return Some((1, 1_000_000));
    }
    None
}

fn truncate_chars(text: String, max_len: Option<usize>) -> String {
    match max_len {
        Some(limit) => text.chars().take(limit).collect(),
        None => text,
    }
}

fn format_unique_string(prefix: &str, ordinal: usize, max_len: Option<usize>) -> String {
    let suffix = ordinal.to_string();
    match max_len {
        None => format!("{prefix}{suffix}"),
        Some(0) => String::new(),
        Some(limit) if limit <= suffix.len() => {
            let start = suffix.len() - limit;
            suffix[start..].to_string()
        }
        Some(limit) => {
            let prefix_budget = limit - suffix.len();
            let trimmed_prefix: String = prefix.chars().take(prefix_budget).collect();
            format!("{trimmed_prefix}{suffix}")
        }
    }
}

fn generate_value(col: &TableColumn, index: usize, is_unique: bool, rng: &mut impl Rng) -> Value {
    let name = col.name.to_lowercase();
    let dt = col.data_type.to_lowercase();
    let max_len = parse_max_char_length(&col.data_type);

    if dt.contains("uuid") {
        return Value::Text(truncate_chars(Uuid::new_v4().to_string(), max_len));
    }

    let is_textual =
        dt.contains("char") || dt.contains("text") || dt.contains("string") || dt.contains("clob");
    if is_textual {
        if name.contains("email") {
            let full = format!("user{}@example.com", index + 1);
            let email = match max_len {
                Some(limit) if full.chars().count() > limit => {
                    let compact = format!("u{}@e.co", index + 1);
                    if compact.chars().count() <= limit {
                        compact
                    } else {
                        format_unique_string("u", index + 1, Some(limit))
                    }
                }
                _ => full,
            };
            return Value::Text(email);
        }
        if name.contains("first") && name.contains("name") {
            let text = if is_unique {
                format_unique_string(&format!("{}_", pick(FIRST_NAMES, rng)), index + 1, max_len)
            } else {
                truncate_chars(pick(FIRST_NAMES, rng), max_len)
            };
            return Value::Text(text);
        }
        if (name.contains("last") || name.contains("sur")) && name.contains("name") {
            let text = if is_unique {
                format_unique_string(&format!("{}_", pick(LAST_NAMES, rng)), index + 1, max_len)
            } else {
                truncate_chars(pick(LAST_NAMES, rng), max_len)
            };
            return Value::Text(text);
        }
        if name.contains("name") {
            let text = if is_unique {
                format_unique_string(&format!("{}_", pick(FIRST_NAMES, rng)), index + 1, max_len)
            } else {
                truncate_chars(
                    format!("{} {}", pick(FIRST_NAMES, rng), pick(LAST_NAMES, rng)),
                    max_len,
                )
            };
            return Value::Text(text);
        }
        if name.contains("phone") {
            let phone = format!("+1{:010}", rng.gen_range(0u64..9_999_999_999));
            return Value::Text(truncate_chars(phone, max_len));
        }
        if name.contains("city") {
            let text = if is_unique {
                format_unique_string(&format!("{}_", pick(CITIES, rng)), index + 1, max_len)
            } else {
                truncate_chars(pick(CITIES, rng), max_len)
            };
            return Value::Text(text);
        }
        if name.contains("country") {
            let text = if is_unique {
                format_unique_string(&format!("{}_", pick(COUNTRIES, rng)), index + 1, max_len)
            } else {
                truncate_chars(pick(COUNTRIES, rng), max_len)
            };
            return Value::Text(text);
        }

        let base = if is_unique {
            format_unique_string(&format!("{}_", pick(WORDS, rng)), index + 1, max_len)
        } else {
            truncate_chars(format!("{}_{}", pick(WORDS, rng), index + 1), max_len)
        };
        return Value::Text(base);
    }

    if dt.contains("bool") {
        return Value::Bool(rng.r#gen());
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
    if let Some((min_v, max_v)) = int_bounds(&dt) {
        if is_unique {
            return Value::Int(index as i64 + 1);
        }
        return Value::Int(rng.gen_range(min_v..=max_v));
    }

    Value::Text(truncate_chars(
        format!("{}_{}", pick(WORDS, rng), index + 1),
        max_len,
    ))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::{ForeignKey, TableIndex};

    fn col(name: &str, data_type: &str, nullable: bool, is_primary_key: bool) -> TableColumn {
        TableColumn {
            name: name.to_string(),
            data_type: data_type.to_string(),
            nullable,
            default_value: None,
            is_primary_key,
            is_auto_increment: false,
        }
    }

    fn unique_index(name: &str, columns: &[&str]) -> TableIndex {
        TableIndex {
            name: name.to_string(),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            is_unique: true,
            is_primary: false,
            index_type: None,
        }
    }

    fn fk(column: &str, table: &str) -> ForeignKey {
        ForeignKey {
            column: column.to_string(),
            referenced_table: table.to_string(),
            referenced_column: "id".to_string(),
            referenced_schema: None,
            referenced_database: None,
            constraint_name: None,
            is_virtual: false,
        }
    }

    #[test]
    fn unique_values_start_after_existing_rows() {
        let columns = vec![
            col("id", "int", false, true),
            col("email", "varchar(64)", false, false),
        ];
        let schema = TableSchema {
            columns: columns.clone(),
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![],
            row_count_estimate: None,
            indexes: vec![unique_index("uq_email", &["email"])],
        };
        let existing = ExistingRows {
            row_count: 40,
            int_max: HashMap::from([("id".to_string(), 120)]),
        };
        let rows = generate_seed_rows(&schema, &columns, &HashMap::new(), &existing, 3).unwrap();
        let ids: Vec<String> = rows.iter().map(|r| value_dedup_key(&r[0])).collect();
        assert_eq!(ids, vec!["121", "122", "123"]);
        assert_eq!(value_dedup_key(&rows[0][1]), "user41@example.com");

        let tiny = vec![col("id", "tinyint", false, true)];
        let tiny_schema = TableSchema {
            columns: tiny.clone(),
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![],
            row_count_estimate: None,
            indexes: vec![],
        };
        let full = ExistingRows {
            row_count: 120,
            int_max: HashMap::from([("id".to_string(), 120)]),
        };
        assert!(generate_seed_rows(&tiny_schema, &tiny, &HashMap::new(), &full, 10).is_err());
    }

    #[test]
    fn composite_primary_key_columns_may_repeat_but_tuples_stay_distinct() {
        let columns = vec![
            col("user_id", "int", false, true),
            col("role_id", "int", false, true),
        ];
        let schema = TableSchema {
            columns: columns.clone(),
            primary_key: Some(vec!["user_id".to_string(), "role_id".to_string()]),
            foreign_keys: vec![fk("user_id", "users"), fk("role_id", "roles")],
            row_count_estimate: None,
            indexes: vec![],
        };
        let fks = HashMap::from([
            (
                "user_id".to_string(),
                (1..=3).map(Value::Int).collect::<Vec<_>>(),
            ),
            (
                "role_id".to_string(),
                (1..=2).map(Value::Int).collect::<Vec<_>>(),
            ),
        ]);
        let rows =
            generate_seed_rows(&schema, &columns, &fks, &ExistingRows::default(), 6).unwrap();
        let tuples: HashSet<String> = rows
            .iter()
            .map(|r| format!("{}-{}", value_dedup_key(&r[0]), value_dedup_key(&r[1])))
            .collect();
        assert_eq!(tuples.len(), 6);

        let err =
            generate_seed_rows(&schema, &columns, &fks, &ExistingRows::default(), 7).unwrap_err();
        assert!(err.contains("composite"));
    }

    #[test]
    fn parses_char_lengths_and_ignores_numeric_precision() {
        assert_eq!(parse_max_char_length("varchar(10)"), Some(10));
        assert_eq!(parse_max_char_length("character varying(32)"), Some(32));
        assert_eq!(parse_max_char_length("CHAR(2)"), Some(2));
        assert_eq!(parse_max_char_length("decimal(10, 2)"), None);
        assert_eq!(parse_max_char_length("tinyint(1)"), None);
        assert_eq!(parse_max_char_length("text"), None);
    }

    #[test]
    fn respects_varchar_length_and_small_int_bounds() {
        let columns = vec![
            col("id", "int", false, true),
            col("tiny", "tinyint", false, false),
            col("small", "smallint", false, false),
            col("code", "varchar(5)", false, false),
            col("email", "varchar(10)", false, false),
        ];
        let schema = TableSchema {
            columns: columns.clone(),
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![],
            row_count_estimate: None,
            indexes: vec![
                unique_index("uq_code", &["code"]),
                unique_index("uq_email", &["email"]),
            ],
        };
        let rows = generate_seed_rows(
            &schema,
            &columns,
            &HashMap::new(),
            &ExistingRows::default(),
            25,
        )
        .unwrap();
        assert_eq!(rows.len(), 25);

        let mut seen_ids = HashSet::new();
        let mut seen_codes = HashSet::new();
        let mut seen_emails = HashSet::new();
        for row in &rows {
            match &row[0] {
                Value::Int(v) => assert!(seen_ids.insert(*v)),
                other => panic!("expected int id, got {other:?}"),
            }
            match &row[1] {
                Value::Int(v) => assert!((1..=127).contains(v)),
                other => panic!("expected tinyint, got {other:?}"),
            }
            match &row[2] {
                Value::Int(v) => assert!((1..=32_767).contains(v)),
                other => panic!("expected smallint, got {other:?}"),
            }
            match &row[3] {
                Value::Text(s) => {
                    assert!(s.chars().count() <= 5, "code too long: {s}");
                    assert!(seen_codes.insert(s.clone()), "duplicate code: {s}");
                }
                other => panic!("expected text code, got {other:?}"),
            }
            match &row[4] {
                Value::Text(s) => {
                    assert!(s.chars().count() <= 10, "email too long: {s}");
                    assert!(seen_emails.insert(s.clone()), "duplicate email: {s}");
                }
                other => panic!("expected text email, got {other:?}"),
            }
        }
    }

    #[test]
    fn enforces_unique_index_columns_and_rejects_empty_not_null_fk() {
        let columns = vec![
            col("user_id", "int", false, false),
            col("first_name", "varchar(30)", false, false),
        ];
        let schema = TableSchema {
            columns: columns.clone(),
            primary_key: None,
            foreign_keys: vec![ForeignKey {
                column: "user_id".to_string(),
                referenced_table: "users".to_string(),
                referenced_column: "id".to_string(),
                referenced_schema: None,
                referenced_database: None,
                constraint_name: None,
                is_virtual: false,
            }],
            row_count_estimate: None,
            indexes: vec![TableIndex {
                name: "uq_first_name".to_string(),
                columns: vec!["first_name".to_string()],
                is_unique: true,
                is_primary: false,
                index_type: None,
            }],
        };

        // Empty FK pool on NOT NULL FK must fail instead of emitting NULL.
        let err = generate_seed_rows(
            &schema,
            &columns,
            &HashMap::new(),
            &ExistingRows::default(),
            5,
        )
        .unwrap_err();
        assert!(err.contains("NOT NULL"));

        let mut fks = HashMap::new();
        fks.insert("user_id".to_string(), vec![Value::Int(42)]);
        let rows =
            generate_seed_rows(&schema, &columns, &fks, &ExistingRows::default(), 30).unwrap();
        let mut names = HashSet::new();
        for row in rows {
            assert!(matches!(row[0], Value::Int(42)));
            if let Value::Text(name) = &row[1] {
                assert!(
                    names.insert(name.clone()),
                    "duplicate unique index value: {name}"
                );
            }
        }
    }
}
