// SPDX-License-Identifier: Apache-2.0

//! CSV Import Tauri Commands
//!
//! Commands for previewing and importing CSV files into database tables.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;
use tracing::instrument;
use uuid::Uuid;

use crate::engine::types::{Namespace, RowData, SessionId, Value};
use crate::interceptor::{Environment, QueryExecutionResult, SafetyAction};

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

fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(SessionId(uuid))
}

// ==================== Types ====================

#[derive(Debug, Deserialize)]
pub struct CsvImportConfig {
    pub delimiter: Option<String>,
    pub has_header: bool,
    pub null_string: Option<String>,
    pub on_conflict: Option<String>,
    /// Maps CSV column index → table column name. Missing entries = skip.
    pub column_mapping: Option<HashMap<usize, String>>,
}

#[derive(Debug, Serialize)]
pub struct CsvPreviewResponse {
    pub detected_delimiter: String,
    pub headers: Vec<String>,
    pub preview_rows: Vec<Vec<String>>,
    pub total_lines: u64,
}

#[derive(Debug, Serialize)]
pub struct ImportResponse {
    pub success: bool,
    pub imported_rows: u64,
    pub failed_rows: u64,
    pub errors: Vec<String>,
    pub execution_time_ms: f64,
}

// ==================== Delimiter Detection ====================

/// Detects the most likely delimiter by checking consistency across sample lines.
fn detect_delimiter(sample: &str) -> u8 {
    let candidates: &[u8] = &[b',', b';', b'\t', b'|'];
    let lines: Vec<&str> = sample.lines().take(10).collect();

    if lines.is_empty() {
        return b',';
    }

    let mut best = b',';
    let mut best_score: i64 = -1;

    for &delim in candidates {
        let counts: Vec<usize> = lines
            .iter()
            .map(|line| line.as_bytes().iter().filter(|&&b| b == delim).count())
            .collect();

        if counts.is_empty() || counts[0] == 0 {
            continue;
        }

        // Score = count if all lines have the same number of delimiters
        let first = counts[0];
        let consistent = counts.iter().all(|&c| c == first);
        let score = if consistent {
            first as i64
        } else {
            // Partial score for inconsistent but present
            (first as i64) / 2
        };

        if score > best_score {
            best_score = score;
            best = delim;
        }
    }

    best
}

fn delimiter_to_string(d: u8) -> String {
    match d {
        b'\t' => "\\t".to_string(),
        _ => String::from(d as char),
    }
}

fn parse_delimiter(s: &str) -> u8 {
    match s {
        "\\t" | "\t" => b'\t',
        s if s.len() == 1 => s.as_bytes()[0],
        _ => b',',
    }
}

// ==================== Preview Command ====================

#[tauri::command]
#[instrument(skip_all, fields(file_path = %file_path))]
pub async fn preview_csv(
    file_path: String,
    delimiter: Option<String>,
    has_header: Option<bool>,
    preview_limit: Option<usize>,
) -> Result<CsvPreviewResponse, String> {
    let content = tokio::fs::read_to_string(&file_path)
        .await
        .map_err(|e| format!("Failed to read file: {}", e))?;

    // Strip UTF-8 BOM if present
    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);

    let delim = match &delimiter {
        Some(d) => parse_delimiter(d),
        None => detect_delimiter(content),
    };

    let has_header = has_header.unwrap_or(true);
    let limit = preview_limit.unwrap_or(5);

    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .has_headers(has_header)
        .flexible(true)
        .from_reader(content.as_bytes());

    let headers: Vec<String> = if has_header {
        rdr.headers()
            .map_err(|e| format!("Failed to read headers: {}", e))?
            .iter()
            .map(|h| h.to_string())
            .collect()
    } else {
        // Read first record to determine column count
        let first = rdr.records().next();
        match first {
            Some(Ok(ref record)) => (0..record.len())
                .map(|i| format!("Column {}", i + 1))
                .collect(),
            _ => vec![],
        }
    };

    // Re-read for preview rows (need to re-create reader for no-header case)
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .has_headers(has_header)
        .flexible(true)
        .from_reader(content.as_bytes());

    let mut preview_rows: Vec<Vec<String>> = Vec::new();
    for result in rdr.records().take(limit) {
        match result {
            Ok(record) => {
                preview_rows.push(record.iter().map(|f| f.to_string()).collect());
            }
            Err(e) => {
                return Err(format!("Failed to parse row: {}", e));
            }
        }
    }

    // Count total lines (approximate)
    let total_lines = content.lines().count() as u64;
    let total_lines = if has_header && total_lines > 0 {
        total_lines - 1
    } else {
        total_lines
    };

    Ok(CsvPreviewResponse {
        detected_delimiter: delimiter_to_string(delim),
        headers,
        preview_rows,
        total_lines,
    })
}

// ==================== Import Command ====================

#[tauri::command]
#[instrument(
    skip(state, config),
    fields(session_id = %session_id, database = %database, schema = ?schema, table = %table)
)]
pub async fn import_csv(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    table: String,
    file_path: String,
    config: CsvImportConfig,
    acknowledged_dangerous: Option<bool>,
) -> Result<ImportResponse, String> {
    let (session_manager, interceptor) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.interceptor),
        )
    };
    let session = parse_session_id(&session_id)?;

    // Check read-only
    let read_only = session_manager
        .is_read_only(session)
        .await
        .map_err(|e| e.to_string())?;
    if read_only {
        return Ok(ImportResponse {
            success: false,
            imported_rows: 0,
            failed_rows: 0,
            errors: vec![READ_ONLY_BLOCKED.to_string()],
            execution_time_ms: 0.0,
        });
    }

    // Check driver supports mutations
    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    if !driver.capabilities().mutations {
        return Ok(ImportResponse {
            success: false,
            imported_rows: 0,
            failed_rows: 0,
            errors: vec![MUTATIONS_NOT_SUPPORTED.to_string()],
            execution_time_ms: 0.0,
        });
    }

    // Interceptor safety check
    let environment = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let interceptor_env = map_environment(&environment);

    let table_ref = if let Some(ref s) = schema {
        format!("{}.{}.{}", database, s, table)
    } else {
        format!("{}.{}", database, table)
    };
    let query_preview = format!("INSERT INTO {} (CSV import)", table_ref);

    let acknowledged = acknowledged_dangerous.unwrap_or(false);
    let interceptor_context = interceptor.build_context(
        &session_id,
        &query_preview,
        driver.driver_id(),
        interceptor_env,
        read_only,
        acknowledged,
        Some(&database),
        None,
        true,
    );

    let safety_result = interceptor.pre_execute(&interceptor_context);
    if !safety_result.allowed {
        interceptor.post_execute(
            &interceptor_context,
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

        return Ok(ImportResponse {
            success: false,
            imported_rows: 0,
            failed_rows: 0,
            errors: vec![error_msg],
            execution_time_ms: 0.0,
        });
    }

    // Read and parse CSV
    let content = tokio::fs::read_to_string(&file_path)
        .await
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);

    let delim = match &config.delimiter {
        Some(d) => parse_delimiter(d),
        None => detect_delimiter(content),
    };

    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .has_headers(config.has_header)
        .flexible(true)
        .from_reader(content.as_bytes());

    // Get CSV headers for mapping
    let csv_headers: Vec<String> = if config.has_header {
        rdr.headers()
            .map_err(|e| format!("Failed to read headers: {}", e))?
            .iter()
            .map(|h| h.to_string())
            .collect()
    } else {
        // Will be determined from first record
        vec![]
    };

    let namespace = Namespace {
        database: database.clone(),
        schema: schema.clone(),
    };
    // Validate column names in mapping (defense in depth: drivers quote identifiers,
    // but reject obviously invalid names early)
    if let Some(ref mapping) = config.column_mapping {
        for (idx, col_name) in mapping {
            if col_name.is_empty() {
                return Err(format!(
                    "Column mapping error: empty column name for CSV index {}",
                    idx
                ));
            }
            if col_name.len() > 128 {
                return Err(format!(
                    "Column mapping error: column name too long for CSV index {} (max 128 chars)",
                    idx
                ));
            }
            if col_name.contains('\0') || col_name.chars().any(|c| c.is_control()) {
                return Err(format!(
                    "Column mapping error: invalid characters in column name for CSV index {}",
                    idx
                ));
            }
        }
    }

    let null_string = config.null_string.unwrap_or_default();
    let abort_on_error = config
        .on_conflict
        .as_deref()
        .unwrap_or("skip")
        != "skip";

    let start_time = std::time::Instant::now();
    let mut imported_rows: u64 = 0;
    let mut failed_rows: u64 = 0;
    let mut errors: Vec<String> = Vec::new();

    // Get table schema to know column names if no mapping provided
    let table_columns: Vec<String> = match driver
        .describe_table(session, &namespace, &table)
        .await
    {
        Ok(schema) => schema.columns.iter().map(|c| c.name.clone()).collect(),
        Err(e) => {
            return Ok(ImportResponse {
                success: false,
                imported_rows: 0,
                failed_rows: 0,
                errors: vec![format!("Failed to describe table: {}", e)],
                execution_time_ms: start_time.elapsed().as_micros() as f64 / 1000.0,
            });
        }
    };

    for (row_idx, result) in rdr.records().enumerate() {
        let record = match result {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("Row {}: parse error: {}", row_idx + 1, e);
                if abort_on_error {
                    errors.push(msg);
                    break;
                }
                errors.push(msg);
                failed_rows += 1;
                continue;
            }
        };

        let mut row_data = RowData::new();

        if let Some(ref mapping) = config.column_mapping {
            // Use explicit mapping: csv_index → table_column_name
            for (csv_idx, table_col) in mapping {
                if let Some(field) = record.get(*csv_idx) {
                    let value = csv_field_to_value(field, &null_string);
                    row_data.columns.insert(table_col.clone(), value);
                }
            }
        } else if config.has_header && !csv_headers.is_empty() {
            // Map by CSV header name matching table column name
            for (i, field) in record.iter().enumerate() {
                if let Some(header) = csv_headers.get(i) {
                    // Only insert if the header matches a table column
                    if table_columns.iter().any(|c| c == header) {
                        let value = csv_field_to_value(field, &null_string);
                        row_data.columns.insert(header.clone(), value);
                    }
                }
            }
        } else {
            // Map by position
            for (i, field) in record.iter().enumerate() {
                if let Some(col_name) = table_columns.get(i) {
                    let value = csv_field_to_value(field, &null_string);
                    row_data.columns.insert(col_name.clone(), value);
                }
            }
        }

        if row_data.columns.is_empty() {
            failed_rows += 1;
            errors.push(format!("Row {}: no columns mapped", row_idx + 1));
            if abort_on_error {
                break;
            }
            continue;
        }

        match driver
            .insert_row(session, &namespace, &table, &row_data)
            .await
        {
            Ok(_) => {
                imported_rows += 1;
            }
            Err(e) => {
                let msg = format!("Row {}: {}", row_idx + 1, e);
                if abort_on_error {
                    errors.push(msg);
                    failed_rows += 1;
                    break;
                }
                errors.push(msg);
                failed_rows += 1;
            }
        }
    }

    let execution_time_ms = start_time.elapsed().as_micros() as f64 / 1000.0;

    // Log to interceptor
    interceptor.post_execute(
        &interceptor_context,
        &QueryExecutionResult {
            success: failed_rows == 0,
            error: if errors.is_empty() {
                None
            } else {
                Some(format!("{} errors during import", errors.len()))
            },
            execution_time_ms,
            row_count: Some(imported_rows as i64),
        },
        false,
        None,
    );

    // Cap errors to avoid huge payloads
    if errors.len() > 50 {
        let total = errors.len();
        errors.truncate(50);
        errors.push(format!("... and {} more errors", total - 50));
    }

    Ok(ImportResponse {
        success: failed_rows == 0 && imported_rows > 0,
        imported_rows,
        failed_rows,
        errors,
        execution_time_ms,
    })
}

/// Converts a CSV field string to a Value, handling null strings and type inference.
fn csv_field_to_value(field: &str, null_string: &str) -> Value {
    if field == null_string || (null_string.is_empty() && field.is_empty()) {
        return Value::Null;
    }

    // Try boolean
    match field.to_lowercase().as_str() {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => {}
    }

    // Try integer
    if let Ok(n) = field.parse::<i64>() {
        return Value::Int(n);
    }

    // Try float
    if let Ok(f) = field.parse::<f64>() {
        return Value::Float(f);
    }

    Value::Text(field.to_string())
}
