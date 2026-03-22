// SPDX-License-Identifier: Apache-2.0

//! Schema Export Tauri Command
//!
//! Exports the full DDL schema (tables, routines, triggers, events)
//! of a database to a .sql file.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use tracing::instrument;
use uuid::Uuid;

use crate::engine::schema_export::generate_create_table_ddl;
use crate::engine::sql_generator::SqlDialect;
use crate::engine::types::{
    CollectionListOptions, CollectionType, Namespace, RoutineListOptions, SessionId,
    TriggerListOptions,
};

fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(SessionId(uuid))
}

#[derive(Debug, Deserialize)]
pub struct SchemaExportOptions {
    pub include_tables: Option<bool>,
    pub include_routines: Option<bool>,
    pub include_triggers: Option<bool>,
    pub include_events: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ExportSchemaResponse {
    pub success: bool,
    pub table_count: u32,
    pub routine_count: u32,
    pub trigger_count: u32,
    pub event_count: u32,
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

    // NoSQL drivers don't support schema export
    if dialect.is_none() {
        return Ok(ExportSchemaResponse {
            success: false,
            table_count: 0,
            routine_count: 0,
            trigger_count: 0,
            event_count: 0,
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
    let include_routines = options.include_routines.unwrap_or(true);
    let include_triggers = options.include_triggers.unwrap_or(true);
    let include_events = options.include_events.unwrap_or(true);

    let mut output = String::new();
    let mut table_count: u32 = 0;
    let mut routine_count: u32 = 0;
    let mut trigger_count: u32 = 0;
    let mut event_count: u32 = 0;

    // Header
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

    // ========== TABLES ==========
    if include_tables {
        let collections = driver
            .list_collections(
                session,
                &namespace,
                CollectionListOptions {
                    search: None,
                    page: None,
                    page_size: Some(10000),
                },
            )
            .await
            .map_err(|e| e.to_string())?;

        // Filter to tables and views only
        let tables: Vec<_> = collections
            .collections
            .iter()
            .filter(|c| {
                matches!(
                    c.collection_type,
                    CollectionType::Table | CollectionType::View | CollectionType::MaterializedView
                )
            })
            .collect();

        if !tables.is_empty() {
            output.push_str("-- ================================================\n");
            output.push_str("-- TABLES\n");
            output.push_str("-- ================================================\n\n");

            for collection in &tables {
                match driver
                    .describe_table(session, &namespace, &collection.name)
                    .await
                {
                    Ok(table_schema) => {
                        let ddl = generate_create_table_ddl(
                            &table_schema,
                            &collection.name,
                            &namespace,
                            dialect,
                        );
                        output.push_str(&ddl);
                        output.push('\n');
                        table_count += 1;
                    }
                    Err(e) => {
                        output.push_str(&format!(
                            "-- ERROR: Failed to describe table {}: {}\n\n",
                            collection.name, e
                        ));
                    }
                }
            }
        }
    }

    // ========== ROUTINES ==========
    if include_routines && driver.supports_routines() {
        let routines = driver
            .list_routines(
                session,
                &namespace,
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
                        &namespace,
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
                        routine_count += 1;
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
                &namespace,
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
                    .get_trigger_definition(session, &namespace, &trigger.name)
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
                        trigger_count += 1;
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
        let events_list_options = crate::engine::types::EventListOptions {
            search: None,
            page: None,
            page_size: Some(10000),
        };

        let events = driver
            .list_events(session, &namespace, events_list_options)
            .await
            .map_err(|e| e.to_string())?;

        if !events.events.is_empty() {
            output.push_str("-- ================================================\n");
            output.push_str("-- EVENTS\n");
            output.push_str("-- ================================================\n\n");

            for event in &events.events {
                match driver
                    .get_event_definition(session, &namespace, &event.name)
                    .await
                {
                    Ok(def) => {
                        output.push_str(&format!("-- Event: {}\n", event.name));
                        output.push_str(&def.definition);
                        if !def.definition.ends_with(';') {
                            output.push(';');
                        }
                        output.push_str("\n\n");
                        event_count += 1;
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

    // Write to file
    let file_size_bytes = output.len() as u64;
    tokio::fs::write(&file_path, &output)
        .await
        .map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(ExportSchemaResponse {
        success: true,
        table_count,
        routine_count,
        trigger_count,
        event_count,
        file_size_bytes,
        error: None,
    })
}
