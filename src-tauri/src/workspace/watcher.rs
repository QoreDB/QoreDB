// SPDX-License-Identifier: Apache-2.0

//! Workspace File Watcher
//!
//! Monitors the active workspace's `.qoredb/` directory for external changes
//! (e.g. `git pull`) and emits granular Tauri events to the frontend.
//! Self-writes by QoreDB are excluded via the WriteRegistry.

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::Emitter;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use super::write_registry::WriteRegistry;

// Tauri event names emitted to the frontend
pub const EVENT_WS_FS_CONNECTIONS: &str = "workspace_fs:connections";
pub const EVENT_WS_FS_QUERIES: &str = "workspace_fs:queries";
pub const EVENT_WS_FS_NOTEBOOKS: &str = "workspace_fs:notebooks";
pub const EVENT_WS_FS_MANIFEST: &str = "workspace_fs:manifest";

const DEBOUNCE_DURATION: Duration = Duration::from_millis(500);

/// Payload emitted to the frontend when workspace files change.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceFsEvent {
    pub category: String,
    pub changed_files: Vec<String>,
}

/// Classify a changed path into a category based on its location relative to `.qoredb/`.
fn classify_path(path: &Path, workspace_root: &Path) -> Option<&'static str> {
    let relative = path.strip_prefix(workspace_root).ok()?;
    let first_component = relative.components().next()?.as_os_str().to_str()?;

    match first_component {
        "connections" => Some("connections"),
        "queries" => Some("queries"),
        "notebooks" => Some("notebooks"),
        _ => {
            // Check if it's workspace.json at the root
            if relative.file_name()?.to_str()? == "workspace.json"
                && relative.components().count() == 1
            {
                Some("manifest")
            } else {
                None
            }
        }
    }
}

/// Map a category to its Tauri event name.
fn category_event_name(category: &str) -> &'static str {
    match category {
        "connections" => EVENT_WS_FS_CONNECTIONS,
        "queries" => EVENT_WS_FS_QUERIES,
        "notebooks" => EVENT_WS_FS_NOTEBOOKS,
        "manifest" => EVENT_WS_FS_MANIFEST,
        _ => EVENT_WS_FS_MANIFEST,
    }
}

/// Check if a notify event kind is relevant (file content change).
fn is_relevant_event(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

/// Check if a path is a JSON file (our workspace data format).
fn is_json_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map_or(false, |ext| ext == "json")
}

/// Start the workspace file watcher background task.
///
/// - `app_handle`: used to emit events to the frontend
/// - `path_rx`: receives the workspace path to watch (None = stop watching)
/// - `write_registry`: used to filter out self-writes
pub fn start_workspace_watcher(
    app_handle: tauri::AppHandle,
    mut path_rx: watch::Receiver<Option<PathBuf>>,
    write_registry: WriteRegistry,
) {
    tauri::async_runtime::spawn(async move {
        let mut _current_watcher: Option<RecommendedWatcher> = None;
        let mut current_path: Option<PathBuf> = None;

        // Channel for bridging sync notify callbacks to async tokio
        let (notify_tx, mut notify_rx) = mpsc::channel::<notify::Event>(256);

        // Debounce accumulators: category → set of changed filenames
        let mut pending: HashMap<String, HashSet<String>> = HashMap::new();
        let mut debounce_deadline: Option<Instant> = None;

        loop {
            let timeout = debounce_deadline
                .map(|d| tokio::time::sleep_until(d))
                .unwrap_or_else(|| tokio::time::sleep(Duration::from_secs(86400)));

            tokio::select! {
                // New workspace path received
                result = path_rx.changed() => {
                    if result.is_err() {
                        // Sender dropped (app shutting down)
                        break;
                    }

                    let new_path = path_rx.borrow().clone();

                    // Stop old watcher
                    _current_watcher = None;
                    current_path = None;
                    pending.clear();
                    debounce_deadline = None;

                    if let Some(ref ws_path) = new_path {
                        // Start new watcher
                        let tx = notify_tx.clone();
                        match RecommendedWatcher::new(
                            move |res: Result<notify::Event, notify::Error>| {
                                if let Ok(event) = res {
                                    let _ = tx.blocking_send(event);
                                }
                            },
                            notify::Config::default()
                                .with_poll_interval(Duration::from_secs(2)),
                        ) {
                            Ok(mut watcher) => {
                                if watcher.watch(ws_path, RecursiveMode::Recursive).is_ok() {
                                    tracing::info!("Workspace watcher started on {}", ws_path.display());
                                    _current_watcher = Some(watcher);
                                    current_path = Some(ws_path.clone());
                                } else {
                                    tracing::warn!("Failed to watch workspace path: {}", ws_path.display());
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to create workspace watcher: {}", e);
                            }
                        }
                    }
                }

                // File system event received
                Some(event) = notify_rx.recv() => {
                    if !is_relevant_event(&event.kind) {
                        continue;
                    }

                    let ws_root = match &current_path {
                        Some(p) => p,
                        None => continue,
                    };

                    for path in &event.paths {
                        if !is_json_file(path) {
                            continue;
                        }

                        if write_registry.is_self_write(path) {
                            continue;
                        }

                        if let Some(category) = classify_path(path, ws_root) {
                            let filename = path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();

                            pending
                                .entry(category.to_string())
                                .or_default()
                                .insert(filename);

                            // Start or extend debounce window
                            debounce_deadline = Some(Instant::now() + DEBOUNCE_DURATION);
                        }
                    }
                }

                // Debounce timer expired — flush pending events
                _ = timeout => {
                    debounce_deadline = None;

                    for (category, files) in pending.drain() {
                        let event_name = category_event_name(&category);
                        let payload = WorkspaceFsEvent {
                            category: category.clone(),
                            changed_files: files.into_iter().collect(),
                        };
                        let _ = app_handle.emit(event_name, &payload);
                        tracing::debug!("Emitted {} with {} files", event_name, payload.changed_files.len());
                    }
                }
            }
        }

        tracing::info!("Workspace watcher stopped");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn classify_connections() {
        let root = PathBuf::from("/project/.qoredb");
        assert_eq!(
            classify_path(&PathBuf::from("/project/.qoredb/connections/conn_abc.json"), &root),
            Some("connections")
        );
    }

    #[test]
    fn classify_queries() {
        let root = PathBuf::from("/project/.qoredb");
        assert_eq!(
            classify_path(&PathBuf::from("/project/.qoredb/queries/library.json"), &root),
            Some("queries")
        );
    }

    #[test]
    fn classify_notebooks() {
        let root = PathBuf::from("/project/.qoredb");
        assert_eq!(
            classify_path(&PathBuf::from("/project/.qoredb/notebooks/analysis.qnb"), &root),
            Some("notebooks")
        );
    }

    #[test]
    fn classify_manifest() {
        let root = PathBuf::from("/project/.qoredb");
        assert_eq!(
            classify_path(&PathBuf::from("/project/.qoredb/workspace.json"), &root),
            Some("manifest")
        );
    }

    #[test]
    fn classify_unknown_is_none() {
        let root = PathBuf::from("/project/.qoredb");
        assert_eq!(
            classify_path(&PathBuf::from("/project/.qoredb/.gitignore"), &root),
            None
        );
        assert_eq!(
            classify_path(&PathBuf::from("/project/.qoredb/contracts/rules.yaml"), &root),
            None
        );
    }

    #[test]
    fn json_file_detection() {
        assert!(is_json_file(Path::new("conn_abc.json")));
        assert!(!is_json_file(Path::new(".gitignore")));
        assert!(!is_json_file(Path::new("README.md")));
    }
}
