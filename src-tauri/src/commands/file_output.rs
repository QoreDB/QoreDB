// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use serde::Deserialize;
use tauri::utils::config::FsScope;
use tauri_plugin_fs::{FsExt, SafeFilePath};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FileScopeEntry {
    Path(PathBuf),
    Object { path: PathBuf },
}

impl FileScopeEntry {
    fn path(&self) -> PathBuf {
        match self {
            Self::Path(path) | Self::Object { path } => path.clone(),
        }
    }
}

/// Uses the same capability and dialog grants as plugin-fs writes. Only the
/// destination is authorized; the private sibling never needs a frontend grant.
#[tauri::command]
pub async fn write_text_file_atomic<R: tauri::Runtime>(
    webview: tauri::Webview<R>,
    path: SafeFilePath,
    contents: String,
) -> Result<(), String> {
    let resolved = webview
        .resolve_command_scope::<FileScopeEntry>("fs", "write_text_file")
        .map_err(|e| e.to_string())?
        .ok_or("File writing is not permitted for this window")?;
    let global = resolved.global_scope();
    let command = resolved.command_scope();
    let scope = tauri::fs::Scope::new(
        &webview,
        &FsScope::Scope {
            allow: global
                .allows()
                .iter()
                .chain(command.allows())
                .map(|e| e.path())
                .collect(),
            deny: global
                .denies()
                .iter()
                .chain(command.denies())
                .map(|e| e.path())
                .collect(),
            require_literal_leading_dot: None,
        },
    )
    .map_err(|e| e.to_string())?;
    let requested = path.into_path().map_err(|e| e.to_string())?;
    let path = &requested;
    // Canonicalize the existing parent even for a new destination, so an alias
    // directory cannot bypass deny rules. Keep publication on that checked path.
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or("An absolute file path is required")?;
    if !path.is_absolute() {
        return Err("An absolute file path is required".into());
    }
    let path = parent
        .canonicalize()
        .map_err(|e| e.to_string())?
        .join(path.file_name().ok_or("A file name is required")?);
    let dialog_scope = webview.fs_scope();
    if [&requested, &path]
        .iter()
        .any(|p| scope.is_forbidden(p) || dialog_scope.is_forbidden(p))
        || ![&requested, &path]
            .iter()
            .any(|p| scope.is_allowed(p) || dialog_scope.is_allowed(p))
    {
        return Err("File path is not permitted".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        qore_service::paths::atomic_write(&path, contents.as_bytes()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder};
    use tauri::webview::InvokeRequest;

    #[test]
    fn ipc_publication_honors_dialog_grants_denials_errors_and_retry() {
        let app = mock_builder()
            .plugin(tauri_plugin_fs::init())
            .invoke_handler(tauri::generate_handler![write_text_file_atomic])
            .build(tauri::generate_context!())
            .unwrap();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("document.qnb");
        std::fs::write(&path, "previous").unwrap();
        let call = |path: &std::path::Path, contents: &str| {
            get_ipc_response(
                &window,
                InvokeRequest {
                    cmd: "write_text_file_atomic".into(),
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    url: window.url().unwrap(),
                    body: tauri::ipc::InvokeBody::Json(
                        serde_json::json!({"path": path, "contents": contents}),
                    ),
                    headers: Default::default(),
                    invoke_key: INVOKE_KEY.into(),
                },
            )
        };
        assert!(call(&path, "unauthorized").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"previous");
        // Native save/open dialogs grant this same dynamic filesystem scope.
        window.fs_scope().allow_file(&path).unwrap();
        call(&path, "complete é").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "complete é");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);

        let blocked = dir.path().join("blocked.json");
        std::fs::create_dir(&blocked).unwrap();
        window.fs_scope().allow_file(&blocked).unwrap();
        assert!(call(&blocked, "replacement").is_err());
        assert!(blocked.is_dir());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
        std::fs::remove_dir(&blocked).unwrap();
        assert!(call(&blocked, "retry").is_ok());
        assert_eq!(std::fs::read(&blocked).unwrap(), b"retry");

        window.fs_scope().forbid_file(&path).unwrap();
        assert!(call(&path, "forbidden").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "complete é");

        // Capability deny rules must win even over a dialog's explicit grant.
        #[cfg(unix)]
        {
            let denied = std::path::Path::new("/etc/qoredb-synthetic-never-written.json");
            window.fs_scope().allow_file(denied).unwrap();
            assert_eq!(
                call(denied, "forbidden").err().unwrap(),
                serde_json::json!("File path is not permitted")
            );
            let alias = dir.path().join("alias");
            std::os::unix::fs::symlink("/etc", &alias).unwrap();
            let aliased = alias.join("qoredb-synthetic-never-written.json");
            window.fs_scope().allow_file(&aliased).unwrap();
            assert_eq!(
                call(&aliased, "forbidden").err().unwrap(),
                serde_json::json!("File path is not permitted")
            );
            let allowed_alias = dir.path().join("allowed-alias");
            std::os::unix::fs::symlink(dir.path(), &allowed_alias).unwrap();
            let new_file = allowed_alias.join("new.json");
            window.fs_scope().allow_file(&new_file).unwrap();
            assert!(call(&new_file, "complete").is_ok());
            assert_eq!(
                std::fs::read(dir.path().join("new.json")).unwrap(),
                b"complete"
            );
        }
    }
}
