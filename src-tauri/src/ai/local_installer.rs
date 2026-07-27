// SPDX-License-Identifier: BUSL-1.1

use std::fs::File;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use futures::StreamExt;
use reqwest::header::{CONTENT_RANGE, RANGE};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use super::local_manifest::{
    ArchiveFormat, LocalArtifact, LocalArtifactManifest, compare_manifest_versions,
};

pub const INSTALL_PROGRESS_EVENT: &str = "ai-local-install-progress";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalInstallPhase {
    Downloading,
    Verifying,
    Installing,
    Completed,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalInstallArtifact {
    Runtime,
    Model,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalInstallProgress {
    pub phase: LocalInstallPhase,
    pub artifact: Option<LocalInstallArtifact>,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub artifact_downloaded_bytes: u64,
    pub artifact_total_bytes: u64,
    pub error: Option<String>,
}

impl LocalInstallProgress {
    pub fn initial(total_bytes: u64) -> Self {
        Self {
            phase: LocalInstallPhase::Downloading,
            artifact: Some(LocalInstallArtifact::Runtime),
            downloaded_bytes: 0,
            total_bytes,
            artifact_downloaded_bytes: 0,
            artifact_total_bytes: 0,
            error: None,
        }
    }
}

pub struct LocalInstallPaths<'a> {
    pub root: &'a Path,
    pub runtime_dir: &'a Path,
    pub runtime_path: &'a Path,
    pub model_path: &'a Path,
}

pub async fn install_artifacts(
    paths: LocalInstallPaths<'_>,
    manifest: &LocalArtifactManifest,
    client: &reqwest::Client,
    cancel: &CancellationToken,
    notify: &(dyn Fn(LocalInstallProgress) + Send + Sync),
) -> Result<(), String> {
    let installed = read_installation_record(paths.root).await;
    if let Some(record) = installed.as_ref() {
        match compare_manifest_versions(&record.manifest_version, &manifest.version)? {
            std::cmp::Ordering::Greater => {
                if paths.runtime_path.is_file() && paths.model_path.is_file() {
                    emit(
                        notify,
                        LocalInstallProgress {
                            phase: LocalInstallPhase::Completed,
                            artifact: None,
                            downloaded_bytes: 0,
                            total_bytes: 0,
                            artifact_downloaded_bytes: 0,
                            artifact_total_bytes: 0,
                            error: None,
                        },
                    );
                    return Ok(());
                }
                return Err(
                    "A newer Qore AI Local installation exists, but its signed manifest is unavailable"
                        .to_string(),
                );
            }
            std::cmp::Ordering::Equal if !record.matches(manifest) => {
                return Err(
                    "Qore AI manifest version was reused with different artifact hashes"
                        .to_string(),
                );
            }
            _ => {}
        }
    }

    let runtime_current = paths.runtime_path.is_file()
        && installed
            .as_ref()
            .is_some_and(|record| record.runtime_sha256 == manifest.runtime.sha256);
    let model_current = paths.model_path.is_file()
        && installed
            .as_ref()
            .is_some_and(|record| record.model_sha256 == manifest.model.sha256);
    let total_bytes = (!runtime_current)
        .then_some(manifest.runtime.size)
        .unwrap_or(0)
        + (!model_current).then_some(manifest.model.size).unwrap_or(0);
    let downloads_dir = paths.root.join(".downloads");
    tokio::fs::create_dir_all(&downloads_dir)
        .await
        .map_err(|error| format!("Could not create the Qore AI download directory: {error}"))?;

    let mut completed_bytes = 0;
    if !runtime_current {
        let archive = download_artifact(
            &downloads_dir,
            &manifest.runtime,
            LocalInstallArtifact::Runtime,
            completed_bytes,
            total_bytes,
            client,
            cancel,
            notify,
        )
        .await?;
        emit(
            notify,
            LocalInstallProgress {
                phase: LocalInstallPhase::Installing,
                artifact: Some(LocalInstallArtifact::Runtime),
                downloaded_bytes: manifest.runtime.size,
                total_bytes,
                artifact_downloaded_bytes: manifest.runtime.size,
                artifact_total_bytes: manifest.runtime.size,
                error: None,
            },
        );
        install_runtime_archive(
            archive.clone(),
            paths.runtime_dir.to_path_buf(),
            manifest.runtime.format,
            &manifest.runtime_relative_path,
        )
        .await?;
        let _ = tokio::fs::remove_file(archive).await;
    }
    if !runtime_current {
        completed_bytes += manifest.runtime.size;
    }

    check_cancelled(cancel)?;
    if !model_current {
        let model_download = download_artifact(
            &downloads_dir,
            &manifest.model,
            LocalInstallArtifact::Model,
            completed_bytes,
            total_bytes,
            client,
            cancel,
            notify,
        )
        .await?;
        emit(
            notify,
            LocalInstallProgress {
                phase: LocalInstallPhase::Installing,
                artifact: Some(LocalInstallArtifact::Model),
                downloaded_bytes: total_bytes,
                total_bytes,
                artifact_downloaded_bytes: manifest.model.size,
                artifact_total_bytes: manifest.model.size,
                error: None,
            },
        );
        install_model_file(&model_download, paths.model_path).await?;
    }

    write_installation_record(paths.root, manifest).await?;
    emit(
        notify,
        LocalInstallProgress {
            phase: LocalInstallPhase::Completed,
            artifact: None,
            downloaded_bytes: total_bytes,
            total_bytes,
            artifact_downloaded_bytes: 0,
            artifact_total_bytes: 0,
            error: None,
        },
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn download_artifact(
    downloads_dir: &Path,
    artifact: &LocalArtifact,
    kind: LocalInstallArtifact,
    completed_bytes: u64,
    total_bytes: u64,
    client: &reqwest::Client,
    cancel: &CancellationToken,
    notify: &(dyn Fn(LocalInstallProgress) + Send + Sync),
) -> Result<PathBuf, String> {
    let partial_path = downloads_dir.join(format!(
        "{}-{}.partial",
        artifact.id,
        &artifact.sha256[..12]
    ));
    let mut offset = tokio::fs::metadata(&partial_path)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if offset > artifact.size {
        tokio::fs::remove_file(&partial_path)
            .await
            .map_err(|error| format!("Could not reset an invalid AI download: {error}"))?;
        offset = 0;
    }

    check_cancelled(cancel)?;
    if offset == artifact.size {
        emit(
            notify,
            LocalInstallProgress {
                phase: LocalInstallPhase::Verifying,
                artifact: Some(kind),
                downloaded_bytes: completed_bytes + offset,
                total_bytes,
                artifact_downloaded_bytes: offset,
                artifact_total_bytes: artifact.size,
                error: None,
            },
        );
        if let Err(error) = verify_sha256(&partial_path, &artifact.sha256, cancel).await {
            let _ = tokio::fs::remove_file(&partial_path).await;
            return Err(error);
        }
        return Ok(partial_path);
    }

    let mut request = client.get(&artifact.url);
    if offset > 0 {
        request = request.header(RANGE, format!("bytes={offset}-"));
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("Could not download {}: {error}", artifact.id))?;
    let final_url_is_allowed = response.url().scheme() == "https"
        || (cfg!(test)
            && response
                .url()
                .host_str()
                .and_then(|host| host.parse::<std::net::IpAddr>().ok())
                .is_some_and(|address| address.is_loopback()));
    if !final_url_is_allowed {
        return Err(format!(
            "Download for {} was redirected to a non-HTTPS endpoint",
            artifact.id
        ));
    }

    let append = offset > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if append {
        let expected_prefix = format!("bytes {offset}-");
        let valid_range = response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with(&expected_prefix));
        if !valid_range {
            return Err(format!("Invalid resume response for {}", artifact.id));
        }
    } else if !response.status().is_success() {
        return Err(format!(
            "Download failed for {} with HTTP {}",
            artifact.id,
            response.status()
        ));
    } else {
        offset = 0;
    }

    let mut options = tokio::fs::OpenOptions::new();
    options.create(true).write(true);
    if append {
        options.append(true);
    } else {
        options.truncate(true);
    }
    let mut file = options
        .open(&partial_path)
        .await
        .map_err(|error| format!("Could not open the AI download file: {error}"))?;
    let mut downloaded = offset;
    let mut last_emitted = offset;
    let mut stream = response.bytes_stream();
    emit_download_progress(
        notify,
        kind,
        completed_bytes,
        total_bytes,
        downloaded,
        artifact.size,
    );

    while let Some(chunk) = tokio::select! {
        _ = cancel.cancelled() => return Err("Qore AI Local installation cancelled".to_string()),
        chunk = stream.next() => chunk,
    } {
        let chunk = chunk.map_err(|error| format!("AI download interrupted: {error}"))?;
        downloaded = downloaded
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| "AI download size overflow".to_string())?;
        if downloaded > artifact.size {
            return Err(format!(
                "Download for {} exceeded its manifest size",
                artifact.id
            ));
        }
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("Could not write the AI download: {error}"))?;
        if downloaded == artifact.size || downloaded.saturating_sub(last_emitted) >= 1024 * 1024 {
            emit_download_progress(
                notify,
                kind,
                completed_bytes,
                total_bytes,
                downloaded,
                artifact.size,
            );
            last_emitted = downloaded;
        }
    }
    file.flush()
        .await
        .map_err(|error| format!("Could not flush the AI download: {error}"))?;
    drop(file);

    if downloaded != artifact.size {
        return Err(format!(
            "Incomplete download for {}: expected {} bytes, received {}",
            artifact.id, artifact.size, downloaded
        ));
    }
    emit(
        notify,
        LocalInstallProgress {
            phase: LocalInstallPhase::Verifying,
            artifact: Some(kind),
            downloaded_bytes: completed_bytes + downloaded,
            total_bytes,
            artifact_downloaded_bytes: downloaded,
            artifact_total_bytes: artifact.size,
            error: None,
        },
    );
    if let Err(error) = verify_sha256(&partial_path, &artifact.sha256, cancel).await {
        let _ = tokio::fs::remove_file(&partial_path).await;
        return Err(error);
    }
    Ok(partial_path)
}

fn emit_download_progress(
    notify: &(dyn Fn(LocalInstallProgress) + Send + Sync),
    kind: LocalInstallArtifact,
    completed_bytes: u64,
    total_bytes: u64,
    downloaded: u64,
    artifact_size: u64,
) {
    emit(
        notify,
        LocalInstallProgress {
            phase: LocalInstallPhase::Downloading,
            artifact: Some(kind),
            downloaded_bytes: completed_bytes + downloaded,
            total_bytes,
            artifact_downloaded_bytes: downloaded,
            artifact_total_bytes: artifact_size,
            error: None,
        },
    );
}

async fn verify_sha256(
    path: &Path,
    expected: &str,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| format!("Could not verify the AI download: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        check_cancelled(cancel)?;
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| format!("Could not verify the AI download: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        return Err(format!(
            "SHA-256 verification failed: expected {expected}, received {actual}"
        ));
    }
    Ok(())
}

async fn install_runtime_archive(
    archive_path: PathBuf,
    runtime_dir: PathBuf,
    format: ArchiveFormat,
    runtime_relative_path: &str,
) -> Result<(), String> {
    let staging = runtime_dir.with_extension("installing");
    let previous = runtime_dir.with_extension("previous");
    remove_dir_if_exists(&staging).await?;
    tokio::fs::create_dir_all(&staging)
        .await
        .map_err(|error| format!("Could not create the runtime staging directory: {error}"))?;

    let staging_for_extract = staging.clone();
    tokio::task::spawn_blocking(move || {
        extract_archive(&archive_path, &staging_for_extract, format)
    })
    .await
    .map_err(|error| format!("Runtime extraction task failed: {error}"))??;

    let executable = staging.join(runtime_relative_path);
    if !executable.is_file() {
        remove_dir_if_exists(&staging).await?;
        return Err("The verified runtime archive does not contain llama-server".to_string());
    }
    set_executable(&executable)?;

    if let Some(parent) = runtime_dir.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("Could not create the runtime directory: {error}"))?;
    }
    remove_dir_if_exists(&previous).await?;
    let had_runtime = runtime_dir.exists();
    if had_runtime {
        tokio::fs::rename(&runtime_dir, &previous)
            .await
            .map_err(|error| format!("Could not stage the previous AI runtime: {error}"))?;
    }
    if let Err(error) = tokio::fs::rename(&staging, &runtime_dir).await {
        if had_runtime {
            let _ = tokio::fs::rename(&previous, &runtime_dir).await;
        }
        return Err(format!(
            "Could not install the AI runtime atomically: {error}"
        ));
    }
    remove_dir_if_exists(&previous).await?;
    Ok(())
}

fn extract_archive(source: &Path, destination: &Path, format: ArchiveFormat) -> Result<(), String> {
    match format {
        ArchiveFormat::TarGz => {
            let file = File::open(source)
                .map_err(|error| format!("Could not open the runtime archive: {error}"))?;
            let mut archive = tar::Archive::new(GzDecoder::new(file));
            for entry in archive
                .entries()
                .map_err(|error| format!("Could not read the runtime archive: {error}"))?
            {
                let mut entry = entry
                    .map_err(|error| format!("Could not read a runtime archive entry: {error}"))?;
                let path = entry
                    .path()
                    .map_err(|error| format!("Invalid runtime archive path: {error}"))?;
                validate_relative_path(&path)?;
                if !entry
                    .unpack_in(destination)
                    .map_err(|error| format!("Could not extract the runtime archive: {error}"))?
                {
                    return Err("Runtime archive attempted to write outside its destination".into());
                }
            }
            Ok(())
        }
        ArchiveFormat::Zip => {
            let file = File::open(source)
                .map_err(|error| format!("Could not open the runtime archive: {error}"))?;
            let mut archive = zip::ZipArchive::new(file)
                .map_err(|error| format!("Could not read the runtime archive: {error}"))?;
            for index in 0..archive.len() {
                let mut entry = archive
                    .by_index(index)
                    .map_err(|error| format!("Could not read a runtime archive entry: {error}"))?;
                let relative = entry
                    .enclosed_name()
                    .ok_or_else(|| "Invalid path in runtime archive".to_string())?;
                validate_relative_path(&relative)?;
                let output = destination.join(relative);
                if entry.is_dir() {
                    std::fs::create_dir_all(&output).map_err(|error| {
                        format!("Could not create a runtime archive directory: {error}")
                    })?;
                } else {
                    if let Some(parent) = output.parent() {
                        std::fs::create_dir_all(parent).map_err(|error| {
                            format!("Could not create a runtime archive directory: {error}")
                        })?;
                    }
                    let mut output_file = File::create(&output).map_err(|error| {
                        format!("Could not create a runtime archive file: {error}")
                    })?;
                    std::io::copy(&mut entry, &mut output_file).map_err(|error| {
                        format!("Could not extract a runtime archive file: {error}")
                    })?;
                    output_file
                        .flush()
                        .map_err(|error| format!("Could not flush a runtime file: {error}"))?;
                }
            }
            Ok(())
        }
        ArchiveFormat::Raw => Err("The runtime artifact must be an archive".to_string()),
    }
}

fn validate_relative_path(path: &Path) -> Result<(), String> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("Unsafe path in runtime archive".to_string());
    }
    Ok(())
}

async fn install_model_file(download: &Path, model_path: &Path) -> Result<(), String> {
    let previous = model_path.with_extension("previous");
    if let Some(parent) = model_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("Could not create the model directory: {error}"))?;
    }
    remove_file_if_exists(&previous).await?;
    let had_model = model_path.exists();
    if had_model {
        tokio::fs::rename(model_path, &previous)
            .await
            .map_err(|error| format!("Could not stage the previous AI model: {error}"))?;
    }
    if let Err(error) = tokio::fs::rename(download, model_path).await {
        if had_model {
            let _ = tokio::fs::rename(&previous, model_path).await;
        }
        return Err(format!(
            "Could not install the AI model atomically: {error}"
        ));
    }
    remove_file_if_exists(&previous).await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InstallationRecord {
    pub manifest_version: String,
    pub runtime_version: String,
    pub runtime_sha256: String,
    pub runtime_license: String,
    pub model_version: String,
    pub model_sha256: String,
    pub model_license: String,
}

impl InstallationRecord {
    pub fn matches(&self, manifest: &LocalArtifactManifest) -> bool {
        self.runtime_sha256 == manifest.runtime.sha256 && self.model_sha256 == manifest.model.sha256
    }
}

pub(crate) async fn read_installation_record(root: &Path) -> Option<InstallationRecord> {
    let content = tokio::fs::read(root.join("installation.json")).await.ok()?;
    serde_json::from_slice(&content).ok()
}

pub(crate) async fn required_download_bytes(
    root: &Path,
    runtime_path: &Path,
    model_path: &Path,
    manifest: &LocalArtifactManifest,
) -> u64 {
    let installed = read_installation_record(root).await;
    let runtime_current = runtime_path.is_file()
        && installed
            .as_ref()
            .is_some_and(|record| record.runtime_sha256 == manifest.runtime.sha256);
    let model_current = model_path.is_file()
        && installed
            .as_ref()
            .is_some_and(|record| record.model_sha256 == manifest.model.sha256);
    (!runtime_current)
        .then_some(manifest.runtime.size)
        .unwrap_or(0)
        + (!model_current).then_some(manifest.model.size).unwrap_or(0)
}

async fn write_installation_record(
    root: &Path,
    manifest: &LocalArtifactManifest,
) -> Result<(), String> {
    let record = InstallationRecord {
        manifest_version: manifest.version.clone(),
        runtime_version: manifest.runtime.version.clone(),
        runtime_sha256: manifest.runtime.sha256.clone(),
        runtime_license: manifest.runtime.license.clone(),
        model_version: manifest.model.version.clone(),
        model_sha256: manifest.model.sha256.clone(),
        model_license: manifest.model.license.clone(),
    };
    let content = serde_json::to_vec_pretty(&record)
        .map_err(|error| format!("Could not serialize the AI installation record: {error}"))?;
    let destination = root.join("installation.json");
    let temporary = root.join("installation.json.tmp");
    let previous = root.join("installation.json.previous");
    tokio::fs::write(&temporary, content)
        .await
        .map_err(|error| format!("Could not write the AI installation record: {error}"))?;
    remove_file_if_exists(&previous).await?;
    let had_record = destination.exists();
    if had_record {
        tokio::fs::rename(&destination, &previous)
            .await
            .map_err(|error| format!("Could not stage the previous AI install record: {error}"))?;
    }
    if let Err(error) = tokio::fs::rename(&temporary, &destination).await {
        if had_record {
            let _ = tokio::fs::rename(&previous, &destination).await;
        }
        return Err(format!(
            "Could not install the AI installation record: {error}"
        ));
    }
    remove_file_if_exists(&previous).await?;
    Ok(())
}

async fn remove_dir_if_exists(path: &Path) -> Result<(), String> {
    match tokio::fs::remove_dir_all(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not clean the AI runtime directory: {error}")),
    }
}

async fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not clean the AI model file: {error}")),
    }
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)
        .map_err(|error| format!("Could not inspect llama-server permissions: {error}"))?
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)
        .map_err(|error| format!("Could not make llama-server executable: {error}"))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn check_cancelled(cancel: &CancellationToken) -> Result<(), String> {
    if cancel.is_cancelled() {
        Err("Qore AI Local installation cancelled".to_string())
    } else {
        Ok(())
    }
}

fn emit(notify: &(dyn Fn(LocalInstallProgress) + Send + Sync), progress: LocalInstallProgress) {
    notify(progress);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::local_manifest::manifest_for_target;
    use wiremock::matchers::{header, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn sha256_verification_accepts_expected_content() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("artifact");
        tokio::fs::write(&path, b"qore-ai").await.unwrap();
        verify_sha256(
            &path,
            "dab4d342d323f59113c04c5f4f5f24ac5f74e454bccc40ae248b8908da6b5042",
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn sha256_verification_rejects_tampered_content() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("artifact");
        tokio::fs::write(&path, b"tampered").await.unwrap();
        let error = verify_sha256(
            &path,
            "dab4d342d323f59113c04c5f4f5f24ac5f74e454bccc40ae248b8908da6b5042",
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert!(error.contains("SHA-256 verification failed"));
    }

    #[test]
    fn archive_paths_cannot_escape_the_staging_directory() {
        assert!(validate_relative_path(Path::new("llama/server")).is_ok());
        assert!(validate_relative_path(Path::new("../outside")).is_err());
        assert!(validate_relative_path(Path::new("/outside")).is_err());
    }

    #[tokio::test]
    async fn interrupted_download_resumes_from_the_partial_file() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("range", "bytes=5-"))
            .respond_with(
                ResponseTemplate::new(206)
                    .insert_header("Content-Range", "bytes 5-6/7")
                    .set_body_bytes(b"ai"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().unwrap();
        let partial = temp.path().join("test-artifact-dab4d342d323.partial");
        tokio::fs::write(&partial, b"qore-").await.unwrap();
        let artifact = LocalArtifact {
            id: "test-artifact".to_string(),
            version: "test".to_string(),
            url: server.uri(),
            size: 7,
            sha256: "dab4d342d323f59113c04c5f4f5f24ac5f74e454bccc40ae248b8908da6b5042".to_string(),
            format: ArchiveFormat::Raw,
            license: "test".to_string(),
        };

        let path = download_artifact(
            temp.path(),
            &artifact,
            LocalInstallArtifact::Model,
            0,
            artifact.size,
            &reqwest::Client::new(),
            &CancellationToken::new(),
            &|_| {},
        )
        .await
        .unwrap();

        assert_eq!(tokio::fs::read(path).await.unwrap(), b"qore-ai");
    }

    #[tokio::test]
    async fn update_size_only_counts_changed_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        let runtime_path = temp.path().join("runtime/llama-server");
        let model_path = temp.path().join("models/model.gguf");
        tokio::fs::create_dir_all(runtime_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::create_dir_all(model_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&runtime_path, b"runtime").await.unwrap();
        tokio::fs::write(&model_path, b"model").await.unwrap();
        let manifest = manifest_for_target(std::env::consts::OS, std::env::consts::ARCH).unwrap();
        write_installation_record(temp.path(), &manifest)
            .await
            .unwrap();
        tokio::fs::remove_file(&model_path).await.unwrap();

        assert_eq!(
            required_download_bytes(temp.path(), &runtime_path, &model_path, &manifest).await,
            manifest.model.size
        );
    }

    #[tokio::test]
    async fn same_manifest_version_cannot_change_artifact_hashes() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = manifest_for_target(std::env::consts::OS, std::env::consts::ARCH).unwrap();
        write_installation_record(temp.path(), &manifest)
            .await
            .unwrap();
        let mut mutated = manifest;
        mutated.model.sha256 = "0".repeat(64);
        let runtime_dir = temp.path().join("runtime");
        let runtime_path = runtime_dir.join("llama-server");
        let model_path = temp.path().join("model.gguf");

        let error = install_artifacts(
            LocalInstallPaths {
                root: temp.path(),
                runtime_dir: &runtime_dir,
                runtime_path: &runtime_path,
                model_path: &model_path,
            },
            &mutated,
            &reqwest::Client::new(),
            &CancellationToken::new(),
            &|_| {},
        )
        .await
        .unwrap_err();
        assert!(error.contains("version was reused"));
    }

    #[tokio::test]
    async fn embedded_fallback_does_not_downgrade_a_newer_installation() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = manifest_for_target(std::env::consts::OS, std::env::consts::ARCH).unwrap();
        let mut installed = manifest.clone();
        installed.version = "2026-07-23.1".to_string();
        write_installation_record(temp.path(), &installed)
            .await
            .unwrap();
        let runtime_dir = temp.path().join("runtime");
        let runtime_path = runtime_dir.join("llama-server");
        let model_path = temp.path().join("model.gguf");
        tokio::fs::create_dir_all(&runtime_dir).await.unwrap();
        tokio::fs::write(&runtime_path, b"runtime").await.unwrap();
        tokio::fs::write(&model_path, b"model").await.unwrap();

        install_artifacts(
            LocalInstallPaths {
                root: temp.path(),
                runtime_dir: &runtime_dir,
                runtime_path: &runtime_path,
                model_path: &model_path,
            },
            &manifest,
            &reqwest::Client::new(),
            &CancellationToken::new(),
            &|_| {},
        )
        .await
        .unwrap();

        assert_eq!(
            read_installation_record(temp.path())
                .await
                .unwrap()
                .manifest_version,
            "2026-07-23.1"
        );
    }

    #[test]
    fn tar_runtime_archive_is_extracted_inside_staging() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("runtime.tar.gz");
        let archive_file = File::create(&archive_path).unwrap();
        let encoder = flate2::write::GzEncoder::new(archive_file, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        let content = b"runtime";
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        archive
            .append_data(&mut header, "llama/llama-server", &content[..])
            .unwrap();
        archive.into_inner().unwrap().finish().unwrap();

        let destination = temp.path().join("staging");
        std::fs::create_dir_all(&destination).unwrap();
        extract_archive(&archive_path, &destination, ArchiveFormat::TarGz).unwrap();
        assert_eq!(
            std::fs::read(destination.join("llama/llama-server")).unwrap(),
            content
        );
    }

    #[test]
    fn zip_runtime_archive_is_extracted_inside_staging() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("runtime.zip");
        let mut archive = zip::ZipWriter::new(File::create(&archive_path).unwrap());
        archive
            .start_file(
                "llama/llama-server",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(b"runtime").unwrap();
        archive.finish().unwrap();

        let destination = temp.path().join("staging");
        std::fs::create_dir_all(&destination).unwrap();
        extract_archive(&archive_path, &destination, ArchiveFormat::Zip).unwrap();
        assert_eq!(
            std::fs::read(destination.join("llama/llama-server")).unwrap(),
            b"runtime"
        );
    }
}
