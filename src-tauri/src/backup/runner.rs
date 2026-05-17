// SPDX-License-Identifier: Apache-2.0

//! Spawn a backup / restore binary and stream its stdout/stderr to the
//! frontend.
//!
//! Each line emitted by the child is forwarded as a `BackupEvent::Log`.
//! When the process exits we emit `BackupEvent::Completed` with the status
//! code. Active jobs are tracked in `ActiveBackups` so the user can cancel a
//! running job from the UI.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::oneshot;
use uuid::Uuid;

use super::args::{
    build_backup_args, build_restore_args, BackupOptions, RestoreOptions,
};
use super::tools::BackupTool;

/// Registry of currently running jobs. Each entry holds the cancel sender for
/// that job. Calling `cancel(job_id)` triggers the running spawn to kill the
/// child and resolve with `success: false`.
#[derive(Default)]
pub struct ActiveBackups {
    inner: Mutex<HashMap<String, oneshot::Sender<()>>>,
}

impl ActiveBackups {
    pub fn new() -> Self {
        Self::default()
    }

    fn register(&self, job_id: String, sender: oneshot::Sender<()>) {
        self.inner.lock().insert(job_id, sender);
    }

    fn deregister(&self, job_id: &str) {
        self.inner.lock().remove(job_id);
    }

    /// Public registration entry point for the in-process DuckDB runner —
    /// mirrors what the spawn-based path uses internally.
    pub fn register_cancel(&self, job_id: String, sender: oneshot::Sender<()>) {
        self.register(job_id, sender);
    }

    pub fn deregister_cancel(&self, job_id: &str) {
        self.deregister(job_id);
    }

    /// Returns `true` when the job was found and the cancel signal was sent.
    pub fn cancel(&self, job_id: &str) -> bool {
        let sender = self.inner.lock().remove(job_id);
        match sender {
            Some(tx) => tx.send(()).is_ok(),
            None => false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackupEvent {
    /// First event emitted before the child produces any output. Lets the
    /// frontend bind a cancel button to the job_id immediately.
    Started { job_id: String },
    /// A line printed by the child process. `stream` is `"stdout"` or `"stderr"`.
    Log { stream: String, line: String },
    /// The job finished. `success` reflects exit code 0; `code` is `None` when
    /// the process was killed by a signal.
    Completed { success: bool, code: Option<i32> },
}

#[derive(Debug)]
pub struct BackupJob {
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct BackupJobOutcome {
    pub job_id: String,
    pub success: bool,
    pub exit_code: Option<i32>,
}

/// Sink that receives each emitted event. The Tauri command wires this to
/// `AppHandle::emit` so the frontend gets a live feed. Required to be
/// `Send + Sync + 'static` so it can be cloned into the streaming tasks.
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, job_id: &str, event: BackupEvent);
}

pub async fn run_backup(
    binary: PathBuf,
    tool: BackupTool,
    opts: BackupOptions,
    redirect_stdout_to: Option<PathBuf>,
    sink: Arc<dyn EventSink>,
    active: Arc<ActiveBackups>,
) -> Result<BackupJobOutcome, String> {
    let (args, env) = build_backup_args(tool, &opts)?;
    spawn_and_stream(binary, args, env, redirect_stdout_to, sink, active).await
}

pub async fn run_restore(
    binary: PathBuf,
    tool: BackupTool,
    opts: RestoreOptions,
    sink: Arc<dyn EventSink>,
    active: Arc<ActiveBackups>,
) -> Result<BackupJobOutcome, String> {
    let (args, env) = build_restore_args(tool, &opts)?;
    spawn_and_stream(binary, args, env, None, sink, active).await
}

async fn spawn_and_stream(
    binary: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
    redirect_stdout_to: Option<PathBuf>,
    sink: Arc<dyn EventSink>,
    active: Arc<ActiveBackups>,
) -> Result<BackupJobOutcome, String> {
    let job_id = Uuid::new_v4().to_string();

    let mut command = Command::new(&binary);
    command.args(&args);
    command.kill_on_drop(true);

    let env_map: HashMap<String, String> = env.into_iter().collect();
    for (key, value) in &env_map {
        command.env(key, value);
    }

    command.stderr(Stdio::piped());

    let stdout_file = match &redirect_stdout_to {
        Some(path) => Some(
            std::fs::File::create(path)
                .map_err(|e| format!("Failed to create output file {:?}: {}", path, e))?,
        ),
        None => None,
    };

    if let Some(file) = stdout_file {
        command.stdout(Stdio::from(file));
    } else {
        command.stdout(Stdio::piped());
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", binary.display(), e))?;

    // Announce the job_id immediately so the frontend can wire a cancel
    // button before any output appears.
    sink.emit(
        &job_id,
        BackupEvent::Started {
            job_id: job_id.clone(),
        },
    );

    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    active.register(job_id.clone(), cancel_tx);

    let stdout_handle = if redirect_stdout_to.is_none() {
        child.stdout.take().map(|out| {
            let job_id = job_id.clone();
            let sink = Arc::clone(&sink);
            tokio::spawn(async move {
                stream_pipe(out, &job_id, "stdout", sink.as_ref()).await;
            })
        })
    } else {
        None
    };

    let stderr_handle = child.stderr.take().map(|err| {
        let job_id = job_id.clone();
        let sink = Arc::clone(&sink);
        tokio::spawn(async move {
            stream_pipe(err, &job_id, "stderr", sink.as_ref()).await;
        })
    });

    let cancelled;
    let status = tokio::select! {
        result = child.wait() => {
            cancelled = false;
            result.map_err(|e| format!("Failed to wait on child: {}", e))?
        }
        _ = cancel_rx => {
            cancelled = true;
            // Best-effort kill; if it has already exited we still need to
            // collect its status.
            let _ = child.start_kill();
            child
                .wait()
                .await
                .map_err(|e| format!("Failed to wait on cancelled child: {}", e))?
        }
    };

    active.deregister(&job_id);

    if let Some(handle) = stdout_handle {
        let _ = handle.await;
    }
    if let Some(handle) = stderr_handle {
        let _ = handle.await;
    }

    if cancelled {
        sink.emit(
            &job_id,
            BackupEvent::Log {
                stream: "stderr".to_string(),
                line: "[qoredb] cancelled by user".to_string(),
            },
        );
    }

    let success = !cancelled && status.success();
    let code = status.code();
    sink.emit(&job_id, BackupEvent::Completed { success, code });

    Ok(BackupJobOutcome {
        job_id,
        success,
        exit_code: code,
    })
}

async fn stream_pipe<R>(reader: R, job_id: &str, stream_name: &str, sink: &dyn EventSink)
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        sink.emit(
            job_id,
            BackupEvent::Log {
                stream: stream_name.to_string(),
                line,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct VecSink(Mutex<Vec<(String, BackupEvent)>>);

    impl EventSink for VecSink {
        fn emit(&self, job_id: &str, event: BackupEvent) {
            self.0.lock().unwrap().push((job_id.into(), event));
        }
    }

    fn new_active() -> Arc<ActiveBackups> {
        Arc::new(ActiveBackups::new())
    }

    #[tokio::test]
    async fn nonzero_exit_reported() {
        let sink: Arc<dyn EventSink> = Arc::new(VecSink(Mutex::new(Vec::new())));
        let outcome = spawn_and_stream(
            PathBuf::from("/bin/sh"),
            vec!["-c".into(), "exit 7".into()],
            Vec::new(),
            None,
            sink,
            new_active(),
        )
        .await
        .unwrap();

        assert!(!outcome.success);
        assert_eq!(outcome.exit_code, Some(7));
    }

    #[tokio::test]
    async fn missing_binary_returns_error() {
        let sink: Arc<dyn EventSink> = Arc::new(VecSink(Mutex::new(Vec::new())));
        let err = spawn_and_stream(
            PathBuf::from("/nonexistent/qoredb-fake-binary"),
            Vec::new(),
            Vec::new(),
            None,
            sink,
            new_active(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("Failed to spawn"));
    }

    #[tokio::test]
    async fn echo_stdout_emits_lines() {
        let inner = Arc::new(VecSink(Mutex::new(Vec::new())));
        let sink: Arc<dyn EventSink> = inner.clone();
        let outcome = spawn_and_stream(
            PathBuf::from("/bin/sh"),
            vec!["-c".into(), "printf 'hello\\nworld\\n'".into()],
            Vec::new(),
            None,
            sink,
            new_active(),
        )
        .await
        .unwrap();

        assert!(outcome.success);
        let events = inner.0.lock().unwrap();
        let logs: Vec<&str> = events
            .iter()
            .filter_map(|(_, e)| match e {
                BackupEvent::Log { line, stream, .. } if stream == "stdout" => Some(line.as_str()),
                _ => None,
            })
            .collect();
        assert!(logs.contains(&"hello"));
        assert!(logs.contains(&"world"));
    }

    #[tokio::test]
    async fn cancel_kills_running_job() {
        let inner = Arc::new(VecSink(Mutex::new(Vec::new())));
        let sink: Arc<dyn EventSink> = inner.clone();
        let active = new_active();

        // Long-running command we'll kill mid-flight.
        let active_for_cancel = Arc::clone(&active);
        let job = tokio::spawn(async move {
            spawn_and_stream(
                PathBuf::from("/bin/sh"),
                vec!["-c".into(), "sleep 30".into()],
                Vec::new(),
                None,
                sink,
                active_for_cancel,
            )
            .await
        });

        // Wait for the job to register itself.
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let job_ids: Vec<String> = inner
                .0
                .lock()
                .unwrap()
                .iter()
                .filter_map(|(_, e)| match e {
                    BackupEvent::Started { job_id } => Some(job_id.clone()),
                    _ => None,
                })
                .collect();
            if let Some(id) = job_ids.first() {
                assert!(active.cancel(id));
                break;
            }
        }

        let outcome = job.await.unwrap().unwrap();
        assert!(!outcome.success);
    }
}
