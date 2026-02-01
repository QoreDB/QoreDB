use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;
use tauri::Emitter;
use uuid::Uuid;

use crate::engine::traits::{DataEngine, StreamEvent};
use crate::engine::types::{ColumnInfo, QueryId, SessionId};
use crate::engine::SessionManager;
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
        config: ExportConfig,
        window: tauri::Window,
    ) -> Result<String, String> {
        if config.query.trim().is_empty() {
            return Err("Query is required for export".to_string());
        }
        if config.output_path.trim().is_empty() {
            return Err("Output path is required for export".to_string());
        }

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

        let export_id = Uuid::new_v4().to_string();
        let cancel = CancellationToken::new();

        {
            let mut jobs = self.jobs.write().await;
            jobs.insert(export_id.clone(), ExportJob { cancel: cancel.clone() });
        }

        let pipeline = Arc::clone(&self);
        let driver_id = driver.driver_id().to_string();

        let export_id_for_task = export_id.clone();
        tokio::spawn(async move {
            let result = run_export_task(
                driver,
                driver_id,
                session_id,
                config,
                export_id_for_task.clone(),
                cancel,
                window,
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

async fn run_export_task(
    driver: Arc<dyn DataEngine>,
    driver_id: String,
    session_id: SessionId,
    config: ExportConfig,
    export_id: String,
    cancel: CancellationToken,
    window: tauri::Window,
) -> Result<(), String> {
    let start_time = Instant::now();
    let mut last_emit = Instant::now();
    let mut rows_exported: u64 = 0;
    let mut columns: Vec<ColumnInfo> = Vec::new();
    let mut state = ExportState::Running;
    let mut error: Option<String> = None;
    let mut cancel_requested = false;

    emit_progress(
        &window,
        ExportProgress {
            export_id: export_id.clone(),
            state: ExportState::Pending,
            rows_exported: 0,
            bytes_written: 0,
            elapsed_ms: 0,
            rows_per_second: None,
            error: None,
        },
    );

    let mut writer = match create_writer(
        config.format.clone(),
        &config.output_path,
        config.include_headers,
        config.table_name.clone(),
        config.namespace.clone(),
        &driver_id,
    )
    .await
    {
        Ok(writer) => writer,
        Err(err) => {
            emit_progress(
                &window,
                build_progress(
                    &export_id,
                    ExportState::Failed,
                    0,
                    0,
                    start_time,
                    Some(err.clone()),
                ),
            );
            return Err(err);
        }
    };

    let (sender, mut receiver) = tokio::sync::mpsc::channel(100);
    let query = config.query.clone();
    let namespace = config.namespace.clone();
    let query_id = QueryId::new();

    let mut driver_task = tokio::spawn({
        let driver = Arc::clone(&driver);
        async move { driver.execute_stream_in_namespace(session_id, namespace, &query, query_id, sender).await }
    });

    emit_progress(
        &window,
        build_progress(&export_id, ExportState::Running, 0, writer.bytes_written(), start_time, None),
    );

    let batch_size = config.batch_size.unwrap_or(1000).max(1) as u64;
    let limit = config.limit;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = driver.cancel(session_id, Some(query_id)).await;
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
                        if let Err(err) = writer.write_row(&columns, &row).await {
                            state = ExportState::Failed;
                            error = Some(err);
                            break;
                        }
                        rows_exported += 1;

                        if rows_exported % batch_size == 0 {
                            if let Err(err) = writer.flush().await {
                                state = ExportState::Failed;
                                error = Some(err);
                                break;
                            }
                        }

                        if let Some(limit) = limit {
                            if rows_exported >= limit {
                                let _ = driver.cancel(session_id, Some(query_id)).await;
                                cancel_requested = true;
                                state = ExportState::Completed;
                                break;
                            }
                        }

                        if last_emit.elapsed() >= Duration::from_millis(250) {
                            emit_progress(
                                &window,
                                build_progress(
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
                        error = Some(err);
                        break;
                    }
                    Some(StreamEvent::Done(_)) => {
                        state = ExportState::Completed;
                        break;
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }

    if matches!(state, ExportState::Cancelled | ExportState::Failed) || cancel_requested {
        if timeout(Duration::from_secs(2), &mut driver_task).await.is_err() {
            driver_task.abort();
        }
    } else if matches!(state, ExportState::Running) {
        match driver_task.await {
            Ok(Ok(())) => state = ExportState::Completed,
            Ok(Err(err)) => {
                state = ExportState::Failed;
                error = Some(err.to_string());
            }
            Err(err) => {
                state = ExportState::Failed;
                error = Some(err.to_string());
            }
        }
    }

    if let Err(err) = writer.flush().await {
        if error.is_none() {
            state = ExportState::Failed;
            error = Some(err);
        }
    }

    if let Err(err) = writer.finish().await {
        if error.is_none() {
            state = ExportState::Failed;
            error = Some(err);
        }
    }

    emit_progress(
        &window,
        build_progress(
            &export_id,
            state,
            rows_exported,
            writer.bytes_written(),
            start_time,
            error,
        ),
    );

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
