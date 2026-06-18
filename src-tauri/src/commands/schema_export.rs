// SPDX-License-Identifier: Apache-2.0

//! Schema Export Tauri Command
//!
//! Exports the full DDL schema (tables, routines, triggers, events)
//! of a database to a .sql file.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;
use tracing::instrument;
use uuid::Uuid;

use crate::engine::schema_export::generate_create_table_ddl;
use crate::engine::sql_generator::SqlDialect;
use crate::engine::traits::DataEngine;
use crate::engine::types::{
    CollectionListOptions, CollectionType, Namespace, RoutineListOptions, SequenceListOptions,
    SessionId, TableSchema, TriggerListOptions,
};

fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(SessionId(uuid))
}

/// A collection (table/view) together with its introspected schema. When
/// `describe_table` fails, `schema` is `None` and `error` carries the message
/// so DDL generation can emit an explanatory comment instead of aborting.
pub(crate) struct DescribedTable {
    pub name: String,
    pub collection_type: CollectionType,
    pub schema: Option<TableSchema>,
    pub error: Option<String>,
}

impl DescribedTable {
    /// Real base table that can receive `INSERT` rows (excludes views).
    pub(crate) fn is_base_table(&self) -> bool {
        matches!(self.collection_type, CollectionType::Table)
    }
}

/// Counts of emitted schema objects, shared between the schema-only and
/// full-database export commands.
#[derive(Default)]
pub(crate) struct SchemaDdlCounts {
    pub table_count: u32,
    pub routine_count: u32,
    pub trigger_count: u32,
    pub event_count: u32,
    pub sequence_count: u32,
}

/// List the tables/views of a namespace and introspect each one. An optional
/// `filter` restricts the result to the named tables (used when the user picks
/// a subset for a full-database export).
pub(crate) async fn list_and_describe_tables(
    driver: &dyn DataEngine,
    session: SessionId,
    namespace: &Namespace,
    filter: Option<&[String]>,
) -> Result<Vec<DescribedTable>, String> {
    let collections = driver
        .list_collections(
            session,
            namespace,
            CollectionListOptions {
                search: None,
                page: None,
                page_size: Some(10000),
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    let mut described = Vec::new();
    for collection in &collections.collections {
        if !matches!(
            collection.collection_type,
            CollectionType::Table | CollectionType::View | CollectionType::MaterializedView
        ) {
            continue;
        }
        if let Some(filter) = filter {
            if !filter.iter().any(|name| name == &collection.name) {
                continue;
            }
        }

        match driver
            .describe_table(session, namespace, &collection.name)
            .await
        {
            Ok(schema) => described.push(DescribedTable {
                name: collection.name.clone(),
                collection_type: collection.collection_type.clone(),
                schema: Some(schema),
                error: None,
            }),
            Err(e) => described.push(DescribedTable {
                name: collection.name.clone(),
                collection_type: collection.collection_type.clone(),
                schema: None,
                error: Some(e.to_string()),
            }),
        }
    }

    Ok(described)
}

/// Build the schema DDL sections (tables, routines, triggers, events,
/// sequences) for a namespace. The `tables` are introspected upfront by the
/// caller so they can be reused (e.g. for the data-ordering pass of a full
/// export). Does not emit a file header — callers prepend their own.
pub(crate) async fn build_schema_ddl(
    driver: &dyn DataEngine,
    session: SessionId,
    namespace: &Namespace,
    dialect: SqlDialect,
    options: &SchemaExportOptions,
    tables: &[DescribedTable],
) -> Result<(String, SchemaDdlCounts), String> {
    let include_tables = options.include_tables.unwrap_or(true);
    let include_routines = options.include_routines.unwrap_or(true);
    let include_triggers = options.include_triggers.unwrap_or(true);
    let include_events = options.include_events.unwrap_or(true);
    let include_sequences = options.include_sequences.unwrap_or(true);

    let mut output = String::new();
    let mut counts = SchemaDdlCounts::default();

    // ========== TABLES ==========
    if include_tables && !tables.is_empty() {
        output.push_str("-- ================================================\n");
        output.push_str("-- TABLES\n");
        output.push_str("-- ================================================\n\n");

        for table in tables {
            match &table.schema {
                Some(table_schema) => {
                    let ddl = generate_create_table_ddl(
                        table_schema,
                        &table.name,
                        namespace,
                        dialect,
                    );
                    output.push_str(&ddl);
                    output.push('\n');
                    counts.table_count += 1;
                }
                None => {
                    output.push_str(&format!(
                        "-- ERROR: Failed to describe table {}: {}\n\n",
                        table.name,
                        table.error.as_deref().unwrap_or("unknown error")
                    ));
                }
            }
        }
    }

    // ========== ROUTINES ==========
    if include_routines && driver.supports_routines() {
        let routines = driver
            .list_routines(
                session,
                namespace,
                RoutineListOptions {
                    search: None,
                    page: None,
                    page_size: Some(10000),
                    routine_type: None,
                },
            )
            .await
            .map_err(|e| e.to_string())?;

        if !routines.routines.is_empty() {
            output.push_str("-- ================================================\n");
            output.push_str("-- FUNCTIONS & PROCEDURES\n");
            output.push_str("-- ================================================\n\n");

            for routine in &routines.routines {
                let args = if routine.arguments.is_empty() {
                    None
                } else {
                    Some(routine.arguments.as_str())
                };

                match driver
                    .get_routine_definition(
                        session,
                        namespace,
                        &routine.name,
                        routine.routine_type.clone(),
                        args,
                    )
                    .await
                {
                    Ok(def) => {
                        output.push_str(&format!(
                            "-- {:?}: {}\n",
                            routine.routine_type, routine.name
                        ));
                        output.push_str(&def.definition);
                        if !def.definition.ends_with(';') {
                            output.push(';');
                        }
                        output.push_str("\n\n");
                        counts.routine_count += 1;
                    }
                    Err(e) => {
                        output.push_str(&format!(
                            "-- ERROR: Failed to get definition for {:?} {}: {}\n\n",
                            routine.routine_type, routine.name, e
                        ));
                    }
                }
            }
        }
    }

    // ========== TRIGGERS ==========
    if include_triggers && driver.supports_triggers() {
        let triggers = driver
            .list_triggers(
                session,
                namespace,
                TriggerListOptions {
                    search: None,
                    page: None,
                    page_size: Some(10000),
                },
            )
            .await
            .map_err(|e| e.to_string())?;

        if !triggers.triggers.is_empty() {
            output.push_str("-- ================================================\n");
            output.push_str("-- TRIGGERS\n");
            output.push_str("-- ================================================\n\n");

            for trigger in &triggers.triggers {
                match driver
                    .get_trigger_definition(session, namespace, &trigger.name)
                    .await
                {
                    Ok(def) => {
                        output.push_str(&format!(
                            "-- Trigger: {} ON {}\n",
                            trigger.name, trigger.table_name
                        ));
                        output.push_str(&def.definition);
                        if !def.definition.ends_with(';') {
                            output.push(';');
                        }
                        output.push_str("\n\n");
                        counts.trigger_count += 1;
                    }
                    Err(e) => {
                        output.push_str(&format!(
                            "-- ERROR: Failed to get definition for trigger {}: {}\n\n",
                            trigger.name, e
                        ));
                    }
                }
            }
        }
    }

    // ========== EVENTS ==========
    if include_events && driver.supports_events() {
        let events = driver
            .list_events(
                session,
                namespace,
                crate::engine::types::EventListOptions {
                    search: None,
                    page: None,
                    page_size: Some(10000),
                },
            )
            .await
            .map_err(|e| e.to_string())?;

        if !events.events.is_empty() {
            output.push_str("-- ================================================\n");
            output.push_str("-- EVENTS\n");
            output.push_str("-- ================================================\n\n");

            for event in &events.events {
                match driver
                    .get_event_definition(session, namespace, &event.name)
                    .await
                {
                    Ok(def) => {
                        output.push_str(&format!("-- Event: {}\n", event.name));
                        output.push_str(&def.definition);
                        if !def.definition.ends_with(';') {
                            output.push(';');
                        }
                        output.push_str("\n\n");
                        counts.event_count += 1;
                    }
                    Err(e) => {
                        output.push_str(&format!(
                            "-- ERROR: Failed to get definition for event {}: {}\n\n",
                            event.name, e
                        ));
                    }
                }
            }
        }
    }

    // ========== SEQUENCES ==========
    if include_sequences && driver.supports_sequences() {
        let sequences = driver
            .list_sequences(
                session,
                namespace,
                SequenceListOptions {
                    search: None,
                    page: None,
                    page_size: Some(10000),
                },
            )
            .await
            .map_err(|e| e.to_string())?;

        if !sequences.sequences.is_empty() {
            output.push_str("-- ================================================\n");
            output.push_str("-- SEQUENCES\n");
            output.push_str("-- ================================================\n\n");

            for seq in &sequences.sequences {
                match driver
                    .get_sequence_definition(session, namespace, &seq.name)
                    .await
                {
                    Ok(def) => {
                        output.push_str(&format!("-- Sequence: {}\n", seq.name));
                        output.push_str(&def.definition);
                        if !def.definition.ends_with(';') {
                            output.push(';');
                        }
                        output.push_str("\n\n");
                        counts.sequence_count += 1;
                    }
                    Err(e) => {
                        output.push_str(&format!(
                            "-- ERROR: Failed to get definition for sequence {}: {}\n\n",
                            seq.name, e
                        ));
                    }
                }
            }
        }
    }

    Ok((output, counts))
}

#[derive(Debug, Deserialize)]
pub struct SchemaExportOptions {
    pub include_tables: Option<bool>,
    pub include_routines: Option<bool>,
    pub include_triggers: Option<bool>,
    pub include_events: Option<bool>,
    pub include_sequences: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ExportSchemaResponse {
    pub success: bool,
    pub table_count: u32,
    pub routine_count: u32,
    pub trigger_count: u32,
    pub event_count: u32,
    pub sequence_count: u32,
    pub file_size_bytes: u64,
    pub error: Option<String>,
}

#[tauri::command]
#[instrument(
    skip(state, options),
    fields(session_id = %session_id, database = %database, schema = ?schema)
)]
pub async fn export_schema(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    file_path: String,
    options: SchemaExportOptions,
) -> Result<ExportSchemaResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    let driver_id = driver.driver_id();
    let dialect = SqlDialect::from_driver_id(driver_id);

    // NoSQL drivers have no DDL dialect.
    if dialect.is_none() {
        return Ok(ExportSchemaResponse {
            success: false,
            table_count: 0,
            routine_count: 0,
            trigger_count: 0,
            event_count: 0,
            sequence_count: 0,
            file_size_bytes: 0,
            error: Some("Schema export is not supported for this driver".to_string()),
        });
    }
    let dialect = dialect.unwrap();

    let namespace = Namespace {
        database: database.clone(),
        schema: schema.clone(),
    };

    let include_tables = options.include_tables.unwrap_or(true);

    let tables = if include_tables {
        list_and_describe_tables(driver.as_ref(), session, &namespace, None).await?
    } else {
        Vec::new()
    };

    let mut output = String::new();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    output.push_str("-- ================================================\n");
    output.push_str("-- QoreDB Schema Export\n");
    output.push_str(&format!("-- Database: {}\n", database));
    if let Some(ref s) = schema {
        output.push_str(&format!("-- Schema: {}\n", s));
    }
    output.push_str(&format!("-- Driver: {}\n", driver_id));
    output.push_str(&format!("-- Date: {}\n", now));
    output.push_str("-- ================================================\n\n");

    let (sections, counts) =
        build_schema_ddl(driver.as_ref(), session, &namespace, dialect, &options, &tables).await?;
    output.push_str(&sections);

    let table_count = counts.table_count;
    let routine_count = counts.routine_count;
    let trigger_count = counts.trigger_count;
    let event_count = counts.event_count;
    let sequence_count = counts.sequence_count;

    // Validate the destination before writing: `file_path` comes from the
    // frontend and `tokio::fs::write` bypasses the Tauri `fs:scope` plugin
    // (cf. audit B6-C4). Without this guard, a forged IPC payload could
    // write `~/.ssh/authorized_keys` or `/etc/...`.
    let resolved = resolve_export_path(&file_path)?;

    let file_size_bytes = output.len() as u64;
    tokio::fs::write(&resolved, &output)
        .await
        .map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(ExportSchemaResponse {
        success: true,
        table_count,
        routine_count,
        trigger_count,
        event_count,
        sequence_count,
        file_size_bytes,
        error: None,
    })
}

/// Whitelist of root directories the frontend may write schema dumps to.
/// Each entry is canonicalised on use so a symlink at `~/Documents` is
/// resolved before the prefix check. Returning an empty `Vec` is fine — the
/// caller will reject any path because no root matches.
fn allowed_export_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(p) = dirs::document_dir() {
        roots.push(p);
    }
    if let Some(p) = dirs::download_dir() {
        roots.push(p);
    }
    if let Some(p) = dirs::desktop_dir() {
        roots.push(p);
    }
    if let Some(mut p) = dirs::data_local_dir() {
        p.push("com.qoredb.app");
        roots.push(p);
    }
    roots
        .into_iter()
        .filter_map(|p| std::fs::canonicalize(&p).ok().or(Some(p)))
        .collect()
}

/// Resolve `requested` (frontend input) to an absolute path located under one
/// of the [`allowed_export_roots`]. Rejects:
/// * relative paths,
/// * paths whose parent directory cannot be canonicalised,
/// * paths that escape the whitelist via `..` once resolved.
pub(crate) fn resolve_export_path(requested: &str) -> Result<PathBuf, String> {
    let trimmed = requested.trim();
    if trimmed.is_empty() {
        return Err("Export path must not be empty".to_string());
    }
    let candidate = PathBuf::from(trimmed);
    if !candidate.is_absolute() {
        return Err(format!("Export path must be absolute, got `{}`", trimmed));
    }

    let parent = candidate
        .parent()
        .ok_or_else(|| format!("Export path `{}` has no parent directory", trimmed))?;
    let parent_canon = std::fs::canonicalize(parent)
        .map_err(|e| format!("Parent directory of `{}` is not accessible: {}", trimmed, e))?;

    let roots = allowed_export_roots();
    if !roots.iter().any(|root| parent_canon.starts_with(root)) {
        return Err(format!(
            "Export path `{}` is outside the allowed locations \
             (Documents, Downloads, Desktop, or app data directory)",
            trimmed
        ));
    }

    // Recombine the canonical parent with the requested filename so we don't
    // accidentally widen the path (e.g. symlinked Documents → /Volumes/Other).
    let filename = candidate
        .file_name()
        .ok_or_else(|| format!("Export path `{}` has no filename", trimmed))?;
    let mut resolved = parent_canon;
    resolved.push(Path::new(filename));
    Ok(resolved)
}
