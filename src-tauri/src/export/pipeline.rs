// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tauri::Emitter;
use tokio::sync::RwLock;
use tokio::time::{Duration, timeout};
use tokio_util::sync::CancellationToken;

use crate::engine::SessionManager;
use crate::engine::traits::{DataEngine, StreamEvent};
use crate::engine::types::{ColumnInfo, QueryId, SessionId};
use crate::export::types::{ExportConfig, ExportFormat, ExportProgress, ExportState};
use crate::export::writers::create_writer;

pub struct ExportPipeline {
    jobs: RwLock<HashMap<String, ExportJob>>,
}

struct ExportJob {
    cancel: CancellationToken,
}

impl ExportPipeline {
    pub fn new() -> Self {
        Self {
            jobs: RwLock::new(HashMap::new()),
        }
    }

    pub async fn start_export(
        self: Arc<Self>,
        session_manager: Arc<SessionManager>,
        session_id: SessionId,
        export_id: String,
        config: ExportConfig,
        window: tauri::Window,
    ) -> Result<String, String> {
        if config.query.trim().is_empty() {
            return Err("Query is required for export".to_string());
        }
        if config.output_path.trim().is_empty() {
            return Err("Output path is required for export".to_string());
        }
        validate_output_path(&config.output_path)?;

        if matches!(config.format, ExportFormat::SqlInsert)
            && config
                .table_name
                .as_deref()
                .map(|name| name.trim().is_empty())
                .unwrap_or(true)
        {
            return Err("Table name is required for SQL INSERT export".to_string());
        }

        let driver = session_manager
            .get_driver(session_id)
            .await
            .map_err(|e| e.to_string())?;

        if !driver.capabilities().streaming {
            return Err("Streaming is not supported by this driver".to_string());
        }

        let cancel = CancellationToken::new();

        {
            let mut jobs = self.jobs.write().await;
            if jobs.contains_key(&export_id) {
                return Err("Export already in progress".to_string());
            }
            jobs.insert(
                export_id.clone(),
                ExportJob {
                    cancel: cancel.clone(),
                },
            );
        }

        let pipeline = Arc::clone(&self);
        let driver_id = driver.driver_id().to_string();
        let masking = session_manager.masking(session_id).await;

        let export_id_for_task = export_id.clone();
        tokio::spawn(async move {
            let result = run_export_task(
                driver,
                driver_id,
                masking,
                session_id,
                config,
                export_id_for_task.clone(),
                cancel,
                move |progress| emit_progress(&window, progress),
            )
            .await;

            if let Err(err) = result {
                tracing::error!("Export {} failed: {}", export_id_for_task, err);
            }

            pipeline.finish_export(&export_id_for_task).await;
        });

        Ok(export_id)
    }

    pub async fn cancel_export(&self, export_id: &str) -> Result<(), String> {
        let jobs = self.jobs.read().await;
        let job = jobs
            .get(export_id)
            .ok_or_else(|| "Export not found".to_string())?;
        job.cancel.cancel();
        Ok(())
    }

    async fn finish_export(&self, export_id: &str) {
        let mut jobs = self.jobs.write().await;
        jobs.remove(export_id);
    }
}

impl Default for ExportPipeline {
    fn default() -> Self {
        Self::new()
    }
}

async fn run_export_task(
    driver: Arc<dyn DataEngine>,
    driver_id: String,
    masking: Option<Arc<qore_core::masking::SessionMasking>>,
    session_id: SessionId,
    config: ExportConfig,
    export_id: String,
    cancel: CancellationToken,
    mut emit: impl FnMut(ExportProgress),
) -> Result<(), String> {
    let start_time = Instant::now();
    let mut last_emit = Instant::now();
    let mut rows_exported: u64 = 0;
    let mut columns: Vec<ColumnInfo> = Vec::new();
    let mut state = ExportState::Running;
    let mut error: Option<String> = None;

    emit(ExportProgress {
        export_id: export_id.clone(),
        state: ExportState::Pending,
        rows_exported: 0,
        bytes_written: 0,
        elapsed_ms: 0,
        rows_per_second: None,
        error: None,
    });

    let output =
        match qore_service::paths::PendingOutput::new(std::path::Path::new(&config.output_path)) {
            Ok(output) => output,
            Err(err) => {
                let message = format!("Failed to prepare export file: {err}");
                emit(build_progress(
                    &export_id,
                    ExportState::Failed,
                    0,
                    0,
                    start_time,
                    Some(message.clone()),
                ));
                return Err(message);
            }
        };
    let mut writer = match create_writer(
        config.format.clone(),
        &output.path().to_string_lossy(),
        config.include_headers,
        config.table_name.clone(),
        config.namespace.clone(),
        &driver_id,
    )
    .await
    {
        Ok(writer) => writer,
        Err(err) => {
            emit(build_progress(
                &export_id,
                ExportState::Failed,
                0,
                0,
                start_time,
                Some(err.clone()),
            ));
            return Err(err);
        }
    };

    let (sender, mut receiver) = tokio::sync::mpsc::channel(100);
    let sender = match masking {
        Some(masking) => qore_core::masking::mask_stream(masking, None, sender),
        None => sender,
    };
    let query = config.query.clone();
    let namespace = config.namespace.clone();
    let query_id = QueryId::new();

    let mut driver_task = tokio::spawn({
        let driver = Arc::clone(&driver);
        async move {
            driver
                .execute_stream_in_namespace(session_id, namespace, &query, query_id, sender)
                .await
        }
    });

    emit(build_progress(
        &export_id,
        ExportState::Running,
        0,
        writer.bytes_written(),
        start_time,
        None,
    ));

    let batch_size = config.batch_size.unwrap_or(1000).max(1) as u64;
    let limit = config.limit;

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                state = ExportState::Cancelled;
                break;
            }
            event = receiver.recv() => {
                match event {
                    Some(StreamEvent::Columns(cols)) => {
                        columns = cols;
                        if let Err(err) = writer.write_header(&columns).await {
                            state = ExportState::Failed;
                            error = Some(err);
                            break;
                        }
                    }
                    Some(StreamEvent::Row(row)) => {
                        if limit == Some(0) {
                            state = ExportState::Completed;
                            break;
                        }
                        if let Err(err) = writer.write_row(&columns, &row).await {
                            state = ExportState::Failed;
                            error = Some(err);
                            break;
                        }
                        rows_exported += 1;

                        if rows_exported.is_multiple_of(batch_size) {
                            if let Err(err) = writer.flush().await {
                                state = ExportState::Failed;
                                error = Some(err);
                                break;
                            }
                        }

                        if let Some(limit) = limit {
                            if rows_exported >= limit {
                                state = ExportState::Completed;
                                break;
                            }
                        }

                        if last_emit.elapsed() >= Duration::from_millis(250) {
                            emit(build_progress(
                                    &export_id,
                                    ExportState::Running,
                                    rows_exported,
                                    writer.bytes_written(),
                                    start_time,
                                    None,
                                ),
                            );
                            last_emit = Instant::now();
                        }
                    }
                    Some(StreamEvent::RowBatch(batch)) => {
                        let mut stop = false;
                        for row in batch {
                            if cancel.is_cancelled() {
                                state = ExportState::Cancelled;
                                stop = true;
                                break;
                            }
                            if limit == Some(0) {
                                state = ExportState::Completed;
                                stop = true;
                                break;
                            }
                            if let Err(err) = writer.write_row(&columns, &row).await {
                                state = ExportState::Failed;
                                error = Some(err);
                                stop = true;
                                break;
                            }
                            rows_exported += 1;

                            if rows_exported.is_multiple_of(batch_size) {
                                if let Err(err) = writer.flush().await {
                                    state = ExportState::Failed;
                                    error = Some(err);
                                    stop = true;
                                    break;
                                }
                            }

                            if let Some(limit) = limit {
                                if rows_exported >= limit {
                                    state = ExportState::Completed;
                                    stop = true;
                                    break;
                                }
                            }
                        }
                        if stop {
                            break;
                        }
                        if last_emit.elapsed() >= Duration::from_millis(250) {
                            emit(build_progress(
                                    &export_id,
                                    ExportState::Running,
                                    rows_exported,
                                    writer.bytes_written(),
                                    start_time,
                                    None,
                                ),
                            );
                            last_emit = Instant::now();
                        }
                    }
                    Some(StreamEvent::Error(err)) => {
                        state = ExportState::Failed;
                        error = Some(qore_core::error::sanitize_error_message(&err));
                        break;
                    }
                    Some(StreamEvent::Done(_)) => {
                        break;
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }

    // Closing the consumer releases producers blocked on a full stream channel.
    drop(receiver);
    if matches!(state, ExportState::Running) {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => state = ExportState::Cancelled,
            result = &mut driver_task => {
                match result {
                    Ok(Ok(())) => state = ExportState::Completed,
                    Ok(Err(err)) => {
                        state = ExportState::Failed;
                        error = Some(err.sanitized_message());
                    }
                    Err(err) => {
                        state = ExportState::Failed;
                        error = Some(err.to_string());
                    }
                }
            }
        }
    }
    if !driver_task.is_finished() {
        if timeout(Duration::from_secs(2), async {
            let _ = driver.cancel(session_id, Some(query_id)).await;
            let _ = (&mut driver_task).await;
        })
        .await
        .is_err()
        {
            driver_task.abort();
            let _ = driver_task.await;
        }
    }

    if matches!(state, ExportState::Completed) {
        if let Err(err) = async {
            writer.flush().await?;
            writer.finish().await
        }
        .await
        {
            state = ExportState::Failed;
            error = Some(err);
        }
    }
    let mut bytes_written = writer.bytes_written();
    drop(writer);
    if cancel.is_cancelled() && matches!(state, ExportState::Completed) {
        state = ExportState::Cancelled;
    }
    if matches!(state, ExportState::Completed) {
        match std::fs::metadata(output.path()) {
            Ok(metadata) => bytes_written = metadata.len(),
            Err(err) => {
                state = ExportState::Failed;
                error = Some(format!("Failed to inspect export file: {err}"));
            }
        }
    }
    if matches!(state, ExportState::Completed) {
        if let Err(err) = output.commit() {
            state = ExportState::Failed;
            error = Some(format!("Failed to publish export file: {err}"));
        }
    } else {
        drop(output);
    }

    emit(build_progress(
        &export_id,
        state,
        rows_exported,
        bytes_written,
        start_time,
        error,
    ));

    Ok(())
}

fn build_progress(
    export_id: &str,
    state: ExportState,
    rows_exported: u64,
    bytes_written: u64,
    start_time: Instant,
    error: Option<String>,
) -> ExportProgress {
    let elapsed_ms = start_time.elapsed().as_millis() as u64;
    let rows_per_second = if elapsed_ms > 0 {
        Some(rows_exported as f64 / (elapsed_ms as f64 / 1000.0))
    } else {
        None
    };

    ExportProgress {
        export_id: export_id.to_string(),
        state,
        rows_exported,
        bytes_written,
        elapsed_ms,
        rows_per_second,
        error,
    }
}

fn emit_progress(window: &tauri::Window, progress: ExportProgress) {
    let _ = window.emit(&format!("export_progress:{}", progress.export_id), progress);
}

/// Validate that an export output path is safe (absolute, no traversal, parent exists).
fn validate_output_path(path: &str) -> Result<(), String> {
    let path = std::path::Path::new(path);

    if !path.is_absolute() {
        return Err("Export output path must be absolute".to_string());
    }

    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err("Export output path must not contain '..' components".to_string());
        }
    }

    if let Some(parent) = path.parent() {
        if !parent.exists() {
            return Err(format!(
                "Parent directory does not exist: {}",
                parent.display()
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testing::MockDriver;
    use crate::engine::types::{Row, Value};

    fn config(path: &std::path::Path) -> ExportConfig {
        ExportConfig {
            query: "SELECT id".into(),
            namespace: None,
            output_path: path.to_string_lossy().into_owned(),
            format: ExportFormat::Json,
            table_name: None,
            include_headers: true,
            batch_size: Some(1),
            limit: None,
        }
    }

    fn stream() -> Vec<StreamEvent> {
        vec![
            StreamEvent::Columns(vec![ColumnInfo {
                name: "id".into(),
                data_type: "BIGINT".into(),
                nullable: false,
                masked: false,
            }]),
            StreamEvent::Row(Row {
                values: vec![Value::Int(9007199254740993)],
            }),
            StreamEvent::Done(1),
        ]
    }

    async fn run_mock(
        path: &std::path::Path,
        events: Vec<StreamEvent>,
        error: Option<String>,
        cancelled: bool,
    ) -> Vec<ExportProgress> {
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.set_stream(events, error);
        let cancel = CancellationToken::new();
        if cancelled {
            cancel.cancel();
        }
        let mut progress = Vec::new();
        run_export_task(
            driver,
            "postgres".into(),
            None,
            SessionId::new(),
            config(path),
            "synthetic".into(),
            cancel,
            |event| progress.push(event),
        )
        .await
        .unwrap();
        progress
    }

    #[tokio::test]
    async fn failed_export_preserves_the_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("export.json");
        std::fs::write(&path, "previous complete export").unwrap();
        let mut events = stream();
        events.pop();
        events.push(StreamEvent::Error("synthetic stream failure".into()));
        let progress = run_mock(&path, events, None, false).await;
        assert_eq!(progress.last().unwrap().state, ExportState::Failed);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "previous complete export"
        );
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no partial file left behind"
        );
    }

    #[tokio::test]
    async fn cancelled_export_does_not_publish_a_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cancelled.json");
        let progress = run_mock(&path, stream(), None, true).await;
        assert_eq!(progress.last().unwrap().state, ExportState::Cancelled);
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn done_event_cannot_hide_a_driver_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("failed.json");
        let progress = run_mock(
            &path,
            stream(),
            Some("synthetic late failure".into()),
            false,
        )
        .await;
        assert_eq!(progress.last().unwrap().state, ExportState::Failed);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn successful_export_replaces_the_destination_with_exact_complete_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("export.json");
        std::fs::write(&path, "previous export").unwrap();
        let progress = run_mock(&path, stream(), None, false).await;
        assert_eq!(progress.last().unwrap().state, ExportState::Completed);
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("9007199254740993"));
        let _: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn row_limits_apply_to_individual_rows_and_batches_including_zero() {
        for batched in [false, true] {
            for limit in [0, 1, 2, 10] {
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("limited.json");
                let mut config = config(&path);
                config.limit = Some(limit);
                let mut events = stream();
                events.truncate(1);
                let rows: Vec<_> = (0..3)
                    .map(|id| Row {
                        values: vec![Value::Int(id)],
                    })
                    .collect();
                if batched {
                    events.push(StreamEvent::RowBatch(rows));
                } else {
                    events.extend(rows.into_iter().map(StreamEvent::Row));
                }
                events.push(StreamEvent::Done(3));
                let driver = Arc::new(MockDriver::new("postgres"));
                driver.set_stream(events, None);
                let mut progress = Vec::new();
                run_export_task(
                    driver,
                    "postgres".into(),
                    None,
                    SessionId::new(),
                    config,
                    "limited".into(),
                    CancellationToken::new(),
                    |event| progress.push(event),
                )
                .await
                .unwrap();
                let last = progress.last().unwrap();
                assert_eq!(last.state, ExportState::Completed);
                assert_eq!(last.rows_exported, limit.min(3));
                let content = std::fs::read(&path).unwrap();
                let rows: Vec<serde_json::Value> = serde_json::from_slice(&content).unwrap();
                assert_eq!(rows.len() as u64, limit.min(3));
                assert_eq!(last.bytes_written, content.len() as u64);
            }
        }
    }

    #[tokio::test]
    async fn cancellation_after_start_preserves_the_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cancelled.json");
        std::fs::write(&path, "previous").unwrap();
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.set_stream(stream(), None);
        let cancel = CancellationToken::new();
        let mut progress = Vec::new();
        run_export_task(
            driver,
            "postgres".into(),
            None,
            SessionId::new(),
            config(&path),
            "cancelled".into(),
            cancel.clone(),
            |event| {
                if event.state == ExportState::Running {
                    cancel.cancel();
                }
                progress.push(event);
            },
        )
        .await
        .unwrap();
        assert_eq!(progress.last().unwrap().state, ExportState::Cancelled);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "previous");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn publication_failure_is_reported_and_staging_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("directory");
        std::fs::create_dir(&path).unwrap();
        let progress = run_mock(&path, stream(), None, false).await;
        assert_eq!(progress.last().unwrap().state, ExportState::Failed);
        assert!(
            progress
                .last()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .contains("publish")
        );
        assert!(path.is_dir());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn failed_exports_preserve_files_for_every_compiled_format() {
        let formats = vec![
            ExportFormat::Csv,
            ExportFormat::Json,
            ExportFormat::Html,
            ExportFormat::SqlInsert,
        ];
        #[cfg(feature = "pro")]
        let formats = [formats, vec![ExportFormat::Xlsx, ExportFormat::Parquet]].concat();
        for format in formats {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("previous.export");
            std::fs::write(&path, "previous").unwrap();
            let mut config = config(&path);
            config.format = format;
            config.table_name = Some("synthetic".into());
            let driver = Arc::new(MockDriver::new("postgres"));
            let mut events = stream();
            events.pop();
            events.push(StreamEvent::Error(
                "postgres://user:synthetic-secret@localhost/db".into(),
            ));
            driver.set_stream(events, None);
            let mut progress = Vec::new();
            run_export_task(
                driver,
                "postgres".into(),
                None,
                SessionId::new(),
                config,
                "failed".into(),
                CancellationToken::new(),
                |event| progress.push(event),
            )
            .await
            .unwrap();
            assert_eq!(progress.last().unwrap().state, ExportState::Failed);
            assert!(
                !progress
                    .last()
                    .unwrap()
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("synthetic-secret")
            );
            assert_eq!(std::fs::read_to_string(path).unwrap(), "previous");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[cfg(feature = "pro")]
    #[tokio::test]
    async fn parquet_conversion_failure_preserves_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("previous.parquet");
        std::fs::write(&path, "previous").unwrap();
        let mut config = config(&path);
        config.format = ExportFormat::Parquet;
        config.batch_size = Some(1000);
        let driver = Arc::new(MockDriver::new("postgres"));
        let mut events = stream();
        events[1] = StreamEvent::Row(Row {
            values: vec![Value::Text("synthetic-secret".into())],
        });
        driver.set_stream(events, None);
        let mut progress = Vec::new();
        run_export_task(
            driver,
            "postgres".into(),
            None,
            SessionId::new(),
            config,
            "failed".into(),
            CancellationToken::new(),
            |event| progress.push(event),
        )
        .await
        .unwrap();
        assert_eq!(progress.last().unwrap().state, ExportState::Failed);
        assert!(
            !progress
                .last()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .contains("synthetic-secret")
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), "previous");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn writer_initialization_failure_preserves_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("previous.export");
        std::fs::write(&path, "previous").unwrap();
        let mut formats = vec![ExportFormat::SqlInsert];
        #[cfg(not(feature = "pro"))]
        formats.extend([ExportFormat::Xlsx, ExportFormat::Parquet]);
        for format in formats.drain(..) {
            let mut config = config(&path);
            config.format = format;
            let driver = Arc::new(MockDriver::new("postgres"));
            driver.set_stream(stream(), None);
            let mut progress = Vec::new();
            assert!(
                run_export_task(
                    driver,
                    "postgres".into(),
                    None,
                    SessionId::new(),
                    config,
                    "failed".into(),
                    CancellationToken::new(),
                    |event| progress.push(event)
                )
                .await
                .is_err()
            );
            assert_eq!(progress.last().unwrap().state, ExportState::Failed);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "previous");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn rejects_relative_export_path() {
        assert!(validate_output_path("data/output.csv").is_err());
    }

    #[test]
    fn rejects_path_with_parent_traversal() {
        assert!(validate_output_path("/home/user/../../../etc/passwd").is_err());
    }

    #[test]
    fn accepts_valid_absolute_path() {
        assert!(validate_output_path("/tmp/export.csv").is_ok());
    }
}

#[cfg(all(test, feature = "pro"))]
#[path = "pipeline_pro_tests.rs"]
mod pro_tests;
