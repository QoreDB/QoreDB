// SPDX-License-Identifier: BUSL-1.1

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;

use super::local_installer::{
    LocalInstallPaths, LocalInstallPhase, LocalInstallProgress, install_artifacts,
};
use super::local_manifest::manifest_for_target;

const MODEL_ID: &str = "qore-qwen3-8b";
const MODEL_FILE: &str = "qwen3-8b-q4_k_m.gguf";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalRuntimeState {
    Unsupported,
    NotInstalled,
    Installing,
    Ready,
    Running,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalRuntimeStatus {
    pub state: LocalRuntimeState,
    pub platform: String,
    pub architecture: String,
    pub runtime_installed: bool,
    pub model_installed: bool,
    pub endpoint: Option<String>,
    pub error: Option<String>,
    pub installation: Option<LocalInstallProgress>,
    pub required_download_bytes: Option<u64>,
}

struct ProcessState {
    child: Child,
    endpoint: String,
}

struct InstallationState {
    cancel: CancellationToken,
    progress: LocalInstallProgress,
}

pub struct LocalAiRuntime {
    root: PathBuf,
    process: Mutex<Option<ProcessState>>,
    installation: Mutex<Option<InstallationState>>,
    last_install_error: Mutex<Option<String>>,
    client: reqwest::Client,
    download_client: reqwest::Client,
}

impl LocalAiRuntime {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            process: Mutex::new(None),
            installation: Mutex::new(None),
            last_install_error: Mutex::new(None),
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(1))
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap_or_default(),
            download_client: reqwest::Client::builder()
                .https_only(true)
                .connect_timeout(Duration::from_secs(20))
                .timeout(Duration::from_secs(60 * 60 * 12))
                .redirect(reqwest::redirect::Policy::limited(10))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn runtime_path(&self) -> PathBuf {
        let runtime_dir = self.runtime_dir();
        match manifest_for_target(std::env::consts::OS, std::env::consts::ARCH) {
            Ok(manifest) => runtime_dir.join(manifest.runtime_relative_path),
            Err(_) => runtime_dir.join(Self::runtime_filename()),
        }
    }

    pub fn model_path(&self) -> PathBuf {
        self.root.join("models").join(MODEL_FILE)
    }

    pub async fn status(&self) -> LocalRuntimeStatus {
        let platform = std::env::consts::OS.to_string();
        let architecture = std::env::consts::ARCH.to_string();
        let manifest = manifest_for_target(&platform, &architecture).ok();
        let required_download_bytes = manifest
            .as_ref()
            .map(|manifest| manifest.runtime.size + manifest.model.size);
        if !Self::is_supported_target() {
            return LocalRuntimeStatus {
                state: LocalRuntimeState::Unsupported,
                platform,
                architecture,
                runtime_installed: false,
                model_installed: false,
                endpoint: None,
                error: None,
                installation: None,
                required_download_bytes: None,
            };
        }

        let runtime_installed = is_executable_candidate(&self.runtime_path());
        let model_installed = self.model_path().is_file();
        let installation = self.current_installation();
        let endpoint = self.current_endpoint();
        if let Some(endpoint) = endpoint {
            if self.is_healthy(&endpoint).await {
                return LocalRuntimeStatus {
                    state: LocalRuntimeState::Running,
                    platform,
                    architecture,
                    runtime_installed,
                    model_installed,
                    endpoint: Some(endpoint),
                    error: None,
                    installation: installation.clone(),
                    required_download_bytes,
                };
            }
            self.clear_finished_process();
        }

        LocalRuntimeStatus {
            state: if installation.is_some() {
                LocalRuntimeState::Installing
            } else if runtime_installed && model_installed {
                LocalRuntimeState::Ready
            } else if self.last_install_error.lock().is_some() {
                LocalRuntimeState::Error
            } else {
                LocalRuntimeState::NotInstalled
            },
            platform,
            architecture,
            runtime_installed,
            model_installed,
            endpoint: None,
            error: self.last_install_error.lock().clone(),
            installation,
            required_download_bytes,
        }
    }

    pub async fn install<F>(&self, notify: F) -> Result<LocalRuntimeStatus, String>
    where
        F: Fn(LocalInstallProgress) + Send + Sync,
    {
        let manifest = manifest_for_target(std::env::consts::OS, std::env::consts::ARCH)?;
        let total_bytes = manifest.runtime.size + manifest.model.size;
        let cancel = CancellationToken::new();
        let initial = LocalInstallProgress::initial(total_bytes);
        {
            let mut installation = self.installation.lock();
            if installation.is_some() {
                return Err("Qore AI Local installation is already running".to_string());
            }
            *installation = Some(InstallationState {
                cancel: cancel.clone(),
                progress: initial.clone(),
            });
        }
        *self.last_install_error.lock() = None;
        self.stop();
        notify(initial);

        let relay = |progress: LocalInstallProgress| {
            if let Some(installation) = self.installation.lock().as_mut() {
                installation.progress = progress.clone();
            }
            notify(progress);
        };
        let runtime_dir = self.runtime_dir();
        let runtime_path = self.runtime_path();
        let model_path = self.model_path();
        let result = install_artifacts(
            LocalInstallPaths {
                root: &self.root,
                runtime_dir: &runtime_dir,
                runtime_path: &runtime_path,
                model_path: &model_path,
            },
            &manifest,
            &self.download_client,
            &cancel,
            &relay,
        )
        .await;

        if let Err(error) = &result {
            let cancelled = cancel.is_cancelled();
            let progress = LocalInstallProgress {
                phase: if cancelled {
                    LocalInstallPhase::Cancelled
                } else {
                    LocalInstallPhase::Error
                },
                artifact: self
                    .current_installation()
                    .and_then(|progress| progress.artifact),
                downloaded_bytes: self
                    .current_installation()
                    .map(|progress| progress.downloaded_bytes)
                    .unwrap_or(0),
                total_bytes,
                artifact_downloaded_bytes: 0,
                artifact_total_bytes: 0,
                error: if cancelled { None } else { Some(error.clone()) },
            };
            notify(progress);
            if !cancelled {
                *self.last_install_error.lock() = Some(error.clone());
            }
        }
        *self.installation.lock() = None;
        result?;
        Ok(self.status().await)
    }

    pub fn cancel_installation(&self) -> bool {
        let installation = self.installation.lock();
        if let Some(installation) = installation.as_ref() {
            installation.cancel.cancel();
            true
        } else {
            false
        }
    }

    pub async fn ensure_running(&self) -> Result<String, String> {
        if !Self::is_supported_target() {
            return Err(format!(
                "Qore AI Local is not supported on {}-{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            ));
        }

        if let Some(endpoint) = self.current_endpoint() {
            if self.is_healthy(&endpoint).await {
                return Ok(endpoint);
            }
            self.clear_finished_process();
        }

        let runtime_path = self.runtime_path();
        let model_path = self.model_path();
        if !is_executable_candidate(&runtime_path) || !model_path.is_file() {
            return Err("Qore AI Local is not installed yet".to_string());
        }

        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|error| format!("Could not reserve a local AI port: {error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| format!("Could not inspect the local AI port: {error}"))?
            .port();
        drop(listener);

        let mut command = Command::new(&runtime_path);
        command
            .arg("--model")
            .arg(&model_path)
            .arg("--alias")
            .arg(MODEL_ID)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("--ctx-size")
            .arg("32768")
            .arg("--jinja")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if let Some(runtime_directory) = runtime_path.parent() {
            command.current_dir(runtime_directory);
            configure_runtime_library_path(&mut command, runtime_directory);
        }
        configure_background_process(&mut command);

        let child = command
            .spawn()
            .map_err(|error| format!("Could not start Qore AI Local: {error}"))?;
        let endpoint = format!("http://127.0.0.1:{port}/v1");
        *self.process.lock() = Some(ProcessState {
            child,
            endpoint: endpoint.clone(),
        });

        for _ in 0..120 {
            if self.is_healthy(&endpoint).await {
                return Ok(endpoint);
            }
            if let Some(error) = self.process_exit_error() {
                return Err(error);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        self.stop();
        Err("Qore AI Local did not become ready within 60 seconds".to_string())
    }

    pub fn stop(&self) {
        if let Some(mut process) = self.process.lock().take() {
            let _ = process.child.start_kill();
        }
    }

    async fn is_healthy(&self, endpoint: &str) -> bool {
        let health = format!("{}/health", endpoint.trim_end_matches("/v1"));
        self.client
            .get(health)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }

    fn current_endpoint(&self) -> Option<String> {
        let guard = self.process.lock();
        guard.as_ref().map(|process| process.endpoint.clone())
    }

    fn current_installation(&self) -> Option<LocalInstallProgress> {
        self.installation
            .lock()
            .as_ref()
            .map(|installation| installation.progress.clone())
    }

    fn runtime_dir(&self) -> PathBuf {
        self.root.join("runtime").join(Self::target_id())
    }

    fn process_exit_error(&self) -> Option<String> {
        let mut guard = self.process.lock();
        let process = guard.as_mut()?;
        match process.child.try_wait() {
            Ok(Some(status)) => {
                *guard = None;
                Some(format!("Qore AI Local stopped during startup ({status})"))
            }
            Ok(None) => None,
            Err(error) => {
                *guard = None;
                Some(format!("Could not inspect Qore AI Local: {error}"))
            }
        }
    }

    fn clear_finished_process(&self) {
        let mut guard = self.process.lock();
        let finished = guard.as_mut().is_some_and(|process| {
            process
                .child
                .try_wait()
                .is_ok_and(|status| status.is_some())
        });
        if finished {
            *guard = None;
        }
    }

    fn is_supported_target() -> bool {
        matches!(std::env::consts::OS, "macos" | "windows" | "linux")
            && matches!(std::env::consts::ARCH, "aarch64" | "x86_64")
    }

    fn target_id() -> String {
        format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
    }

    fn runtime_filename() -> &'static str {
        if cfg!(windows) {
            "llama-server.exe"
        } else {
            "llama-server"
        }
    }
}

impl Drop for LocalAiRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

fn is_executable_candidate(path: &Path) -> bool {
    path.is_file()
}

#[cfg(windows)]
fn configure_background_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn configure_background_process(_command: &mut Command) {}

#[cfg(target_os = "linux")]
fn configure_runtime_library_path(command: &mut Command, runtime_directory: &Path) {
    command.env("LD_LIBRARY_PATH", runtime_directory);
}

#[cfg(target_os = "macos")]
fn configure_runtime_library_path(command: &mut Command, runtime_directory: &Path) {
    command.env("DYLD_LIBRARY_PATH", runtime_directory);
}

#[cfg(windows)]
fn configure_runtime_library_path(_command: &mut Command, _runtime_directory: &Path) {}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn configure_runtime_library_path(_command: &mut Command, _runtime_directory: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_target_scoped_and_models_are_shared() {
        let runtime = LocalAiRuntime::new(PathBuf::from("/tmp/qore-ai"));
        assert!(
            runtime
                .runtime_path()
                .to_string_lossy()
                .contains(SelfTarget::OS)
        );
        assert!(
            runtime
                .runtime_path()
                .to_string_lossy()
                .contains(SelfTarget::ARCH)
        );
        assert!(runtime.model_path().ends_with(MODEL_FILE));
    }

    struct SelfTarget;

    impl SelfTarget {
        const OS: &'static str = std::env::consts::OS;
        const ARCH: &'static str = std::env::consts::ARCH;
    }
}
