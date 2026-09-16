// SPDX-License-Identifier: Apache-2.0

//! Locates the `qore-mcp` binary for the Settings "AI agents" screen.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct McpBinaryStatus {
    pub path: Option<String>,
    pub version: Option<String>,
}

const BINARY: &str = if cfg!(windows) {
    "qore-mcp.exe"
} else {
    "qore-mcp"
};

fn candidates() -> Vec<PathBuf> {
    // A running AppImage mounts itself under a temporary directory, so a path
    // inside it stops working once the app exits: PATH comes first there.
    let in_appimage = std::env::var_os("APPIMAGE").is_some();
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(BINARY)));
    let on_path: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join(BINARY))
                .collect()
        })
        .unwrap_or_default();

    let mut out = Vec::new();
    if !in_appimage {
        out.extend(beside_exe.clone());
    }
    out.extend(on_path);
    if in_appimage {
        out.extend(beside_exe);
    }
    out
}

fn read_version(path: &Path) -> Option<String> {
    let output = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .last()
        .map(str::to_string)
}

fn locate() -> McpBinaryStatus {
    let Some(path) = candidates().into_iter().find(|p| p.is_file()) else {
        return McpBinaryStatus {
            path: None,
            version: None,
        };
    };
    McpBinaryStatus {
        version: read_version(&path),
        path: Some(path.to_string_lossy().into_owned()),
    }
}

#[tauri::command]
pub async fn agents_mcp_status() -> McpBinaryStatus {
    tauri::async_runtime::spawn_blocking(locate)
        .await
        .unwrap_or(McpBinaryStatus {
            path: None,
            version: None,
        })
}
