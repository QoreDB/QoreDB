// SPDX-License-Identifier: Apache-2.0

//! Native DuckDB backup / restore via the in-process driver.
//!
//! DuckDB ships `EXPORT DATABASE 'dir' (FORMAT PARQUET|CSV)` and
//! `IMPORT DATABASE 'dir'` as first-class SQL statements, so we don't need an
//! external binary — we just open the file with the bundled `duckdb` crate and
//! run the statement. The output of a DuckDB export is a **directory** (it
//! contains `schema.sql`, `load.sql`, and one file per table), so the
//! file-picker on the frontend must allow directory selection.

use std::sync::Arc;

use duckdb::Connection;
use tokio::sync::oneshot;
use tokio::task;
use uuid::Uuid;

use super::path_to_string;

use super::args::{BackupFormat, BackupMode, BackupOptions, RestoreOptions};
use super::runner::{ActiveBackups, BackupEvent, BackupJobOutcome, EventSink};

fn export_format(opts: &BackupOptions) -> &'static str {
    match opts.format {
        BackupFormat::Sql => "CSV",
        BackupFormat::PostgresCustom | BackupFormat::MongoArchive => "PARQUET",
    }
}

/// Run a DuckDB backup by opening the database file and issuing
/// `EXPORT DATABASE`. Reuses the same event / outcome shape as the external
/// CLI runner so the frontend doesn't need a separate code path.
pub async fn run_duckdb_backup(
    opts: BackupOptions,
    sink: Arc<dyn EventSink>,
    active: Arc<ActiveBackups>,
) -> Result<BackupJobOutcome, String> {
    let job_id = Uuid::new_v4().to_string();
    sink.emit(
        &job_id,
        BackupEvent::Started {
            job_id: job_id.clone(),
        },
    );

    if matches!(opts.mode, BackupMode::DataOnly | BackupMode::SchemaOnly) {
        let msg = "DuckDB EXPORT DATABASE always writes schema + data; \
                   schema-only / data-only modes are not supported in v1.";
        sink.emit(
            &job_id,
            BackupEvent::Log {
                stream: "stderr".into(),
                line: msg.into(),
            },
        );
        sink.emit(
            &job_id,
            BackupEvent::Completed {
                success: false,
                code: None,
            },
        );
        return Ok(BackupJobOutcome {
            job_id,
            success: false,
            exit_code: None,
        });
    }

    let db_path = opts
        .database
        .as_ref()
        .filter(|d| !d.is_empty())
        .cloned()
        .ok_or_else(|| "Database file path is required for DuckDB backup".to_string())?;

    let output_path = path_to_string(&opts.output_path)?;
    if !is_safe_export_path(&output_path) {
        return Err(format!("Unsafe export path: {output_path:?}"));
    }
    let format = export_format(&opts);
    let sql = format!(
        "EXPORT DATABASE '{}' (FORMAT {format})",
        escape_sql_literal(&output_path),
    );

    sink.emit(
        &job_id,
        BackupEvent::Log {
            stream: "stdout".into(),
            line: format!("EXPORT DATABASE → {output_path} (format={format})"),
        },
    );

    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    active.register_cancel(job_id.clone(), cancel_tx);
    let job_id_for_task = job_id.clone();

    let handle = task::spawn_blocking(move || -> Result<(), String> {
        let conn =
            Connection::open(&db_path).map_err(|e| format!("DuckDB open({db_path}): {e}"))?;
        conn.execute_batch(&sql)
            .map_err(|e| format!("DuckDB EXPORT failed: {e}"))?;
        Ok(())
    });

    let result = tokio::select! {
        res = handle => res.map_err(|e| format!("Backup task panicked: {e}"))?,
        _ = cancel_rx => Err("Cancelled by user".to_string()),
    };
    active.deregister_cancel(&job_id_for_task);

    let success = result.is_ok();
    if let Err(ref msg) = result {
        sink.emit(
            &job_id,
            BackupEvent::Log {
                stream: "stderr".into(),
                line: msg.clone(),
            },
        );
    }
    sink.emit(
        &job_id,
        BackupEvent::Completed {
            success,
            code: if success { Some(0) } else { None },
        },
    );
    Ok(BackupJobOutcome {
        job_id,
        success,
        exit_code: if success { Some(0) } else { None },
    })
}

pub async fn run_duckdb_restore(
    opts: RestoreOptions,
    sink: Arc<dyn EventSink>,
    active: Arc<ActiveBackups>,
) -> Result<BackupJobOutcome, String> {
    let job_id = Uuid::new_v4().to_string();
    sink.emit(
        &job_id,
        BackupEvent::Started {
            job_id: job_id.clone(),
        },
    );

    let db_path = opts
        .database
        .as_ref()
        .filter(|d| !d.is_empty())
        .cloned()
        .ok_or_else(|| "Database file path is required for DuckDB restore".to_string())?;

    let input_path = path_to_string(&opts.input_path)?;
    if !is_safe_export_path(&input_path) {
        return Err(format!("Unsafe import path: {input_path:?}"));
    }
    let sql = format!("IMPORT DATABASE '{}'", escape_sql_literal(&input_path));

    sink.emit(
        &job_id,
        BackupEvent::Log {
            stream: "stdout".into(),
            line: format!("IMPORT DATABASE ← {input_path}"),
        },
    );

    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    active.register_cancel(job_id.clone(), cancel_tx);
    let job_id_for_task = job_id.clone();

    let handle = task::spawn_blocking(move || -> Result<(), String> {
        let conn =
            Connection::open(&db_path).map_err(|e| format!("DuckDB open({db_path}): {e}"))?;
        conn.execute_batch(&sql)
            .map_err(|e| format!("DuckDB IMPORT failed: {e}"))?;
        Ok(())
    });

    let result = tokio::select! {
        res = handle => res.map_err(|e| format!("Restore task panicked: {e}"))?,
        _ = cancel_rx => Err("Cancelled by user".to_string()),
    };
    active.deregister_cancel(&job_id_for_task);

    let success = result.is_ok();
    if let Err(ref msg) = result {
        sink.emit(
            &job_id,
            BackupEvent::Log {
                stream: "stderr".into(),
                line: msg.clone(),
            },
        );
    }
    sink.emit(
        &job_id,
        BackupEvent::Completed {
            success,
            code: if success { Some(0) } else { None },
        },
    );
    Ok(BackupJobOutcome {
        job_id,
        success,
        exit_code: if success { Some(0) } else { None },
    })
}

/// Escapes single quotes for inclusion in a DuckDB SQL string literal.
/// DuckDB doubles single quotes the standard SQL way.
fn escape_sql_literal(s: &str) -> String {
    s.replace('\'', "''")
}

fn is_safe_export_path(s: &str) -> bool {
    !s.chars().any(|c| c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn escape_sql_literal_doubles_quotes() {
        assert_eq!(escape_sql_literal("a'b"), "a''b");
        assert_eq!(escape_sql_literal("plain"), "plain");
    }

    #[test]
    fn safe_path_rejects_control_chars() {
        assert!(is_safe_export_path("/tmp/normal/path"));
        assert!(!is_safe_export_path("/tmp/with\nnewline"));
        assert!(!is_safe_export_path("/tmp/with\x00null"));
    }

    #[test]
    fn export_format_maps_sql_to_csv() {
        let mut o = sample_opts();
        o.format = BackupFormat::Sql;
        assert_eq!(export_format(&o), "CSV");
    }

    #[test]
    fn export_format_defaults_to_parquet_otherwise() {
        let mut o = sample_opts();
        o.format = BackupFormat::PostgresCustom;
        assert_eq!(export_format(&o), "PARQUET");
        o.format = BackupFormat::MongoArchive;
        assert_eq!(export_format(&o), "PARQUET");
    }

    fn sample_opts() -> BackupOptions {
        BackupOptions {
            driver: "duckdb".into(),
            mode: BackupMode::Full,
            format: BackupFormat::Sql,
            host: String::new(),
            port: 0,
            username: None,
            password: None,
            database: Some("/tmp/test.duckdb".into()),
            tables: Vec::new(),
            output_path: PathBuf::from("/tmp/out"),
        }
    }
}
