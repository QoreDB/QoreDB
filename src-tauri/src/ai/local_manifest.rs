// SPDX-License-Identifier: BUSL-1.1

//! Signed artifact catalog for Qore AI Local.
//!
//! The bundled catalog is the offline trust fallback. A newer catalog may be
//! loaded from the project-owned rolling release only after its byte-exact
//! Minisign signature has been verified with QoreDB's updater public key.

use std::collections::HashSet;
use std::path::{Component, Path};

use base64::Engine;
use futures::StreamExt;
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};

const MANIFEST_SCHEMA_VERSION: u32 = 1;
const EMBEDDED_MANIFEST: &[u8] = include_bytes!("../../resources/qore-ai-local-manifest-v1.json");
const REMOTE_MANIFEST_URL: &str = "https://github.com/QoreDB/QoreDB/releases/download/qore-ai-local/qore-ai-local-manifest-v1.json";
const MAX_MANIFEST_BYTES: usize = 256 * 1024;
const MAX_SIGNATURE_BYTES: usize = 16 * 1024;
const MAX_ARTIFACT_BYTES: u64 = 32 * 1024 * 1024 * 1024;

const SUPPORTED_TARGETS: [(&str, &str); 6] = [
    ("macos", "aarch64"),
    ("macos", "x86_64"),
    ("windows", "aarch64"),
    ("windows", "x86_64"),
    ("linux", "aarch64"),
    ("linux", "x86_64"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat {
    Raw,
    TarGz,
    Zip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalArtifact {
    pub id: String,
    pub version: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub format: ArchiveFormat,
    pub license: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalArtifactManifest {
    pub version: String,
    pub runtime: LocalArtifact,
    pub runtime_relative_path: String,
    pub model: LocalArtifact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalManifestSource {
    Embedded,
    Remote,
}

#[derive(Debug, Clone)]
pub struct ResolvedLocalManifest {
    pub manifest: LocalArtifactManifest,
    pub source: LocalManifestSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalManifestCatalog {
    schema_version: u32,
    version: String,
    model: LocalArtifact,
    targets: Vec<LocalTargetManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalTargetManifest {
    os: String,
    architecture: String,
    runtime: LocalArtifact,
    runtime_relative_path: String,
}

pub fn manifest_for_target(os: &str, arch: &str) -> Result<LocalArtifactManifest, String> {
    embedded_catalog()?.for_target(os, arch)
}

pub async fn resolve_manifest_for_target(
    client: &reqwest::Client,
    os: &str,
    arch: &str,
) -> Result<ResolvedLocalManifest, String> {
    let embedded = embedded_catalog()?;
    let embedded_manifest = embedded.for_target(os, arch)?;

    let public_key = updater_public_key_document()?;
    match fetch_signed_catalog(client, REMOTE_MANIFEST_URL, &public_key).await {
        Ok(remote)
            if compare_manifest_versions(&remote.version, &embedded.version)?
                == std::cmp::Ordering::Greater =>
        {
            let manifest = remote.for_target(os, arch)?;
            Ok(ResolvedLocalManifest {
                manifest,
                source: LocalManifestSource::Remote,
            })
        }
        Ok(_) => Ok(ResolvedLocalManifest {
            manifest: embedded_manifest,
            source: LocalManifestSource::Embedded,
        }),
        Err(error) => {
            tracing::warn!("Qore AI remote manifest unavailable; using embedded catalog: {error}");
            Ok(ResolvedLocalManifest {
                manifest: embedded_manifest,
                source: LocalManifestSource::Embedded,
            })
        }
    }
}

pub fn compare_manifest_versions(left: &str, right: &str) -> Result<std::cmp::Ordering, String> {
    Ok(parse_manifest_version(left)?.cmp(&parse_manifest_version(right)?))
}

impl LocalManifestCatalog {
    fn for_target(&self, os: &str, arch: &str) -> Result<LocalArtifactManifest, String> {
        let target = self
            .targets
            .iter()
            .find(|target| target.os == os && target.architecture == arch)
            .ok_or_else(|| format!("Unsupported Qore AI Local target: {os}-{arch}"))?;
        Ok(LocalArtifactManifest {
            version: self.version.clone(),
            runtime: target.runtime.clone(),
            runtime_relative_path: target.runtime_relative_path.clone(),
            model: self.model.clone(),
        })
    }
}

fn embedded_catalog() -> Result<LocalManifestCatalog, String> {
    parse_and_validate_catalog(EMBEDDED_MANIFEST, false)
}

async fn fetch_signed_catalog(
    client: &reqwest::Client,
    manifest_url: &str,
    public_key_document: &str,
) -> Result<LocalManifestCatalog, String> {
    let signature_url = format!("{manifest_url}.sig");
    let (manifest, signature) = tokio::try_join!(
        fetch_limited(client, manifest_url, MAX_MANIFEST_BYTES),
        fetch_limited(client, &signature_url, MAX_SIGNATURE_BYTES)
    )?;
    verify_minisign(&manifest, &signature, public_key_document)?;
    parse_and_validate_catalog(&manifest, true)
}

async fn fetch_limited(
    client: &reqwest::Client,
    url: &str,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Could not fetch signed AI manifest data: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Signed AI manifest request failed with HTTP {}",
            response.status()
        ));
    }
    let final_url_is_allowed = response.url().scheme() == "https"
        || (cfg!(test)
            && response
                .url()
                .host_str()
                .and_then(|host| host.parse::<std::net::IpAddr>().ok())
                .is_some_and(|address| address.is_loopback()));
    if !final_url_is_allowed {
        return Err("Signed AI manifest redirected to a non-HTTPS endpoint".to_string());
    }
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err("Signed AI manifest response is too large".to_string());
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| format!("Signed AI manifest download interrupted: {error}"))?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err("Signed AI manifest response is too large".to_string());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn verify_minisign(
    manifest: &[u8],
    signature_document: &[u8],
    public_key_document: &str,
) -> Result<(), String> {
    let signature_text = decode_signature_document(signature_document)?;
    let public_key = PublicKey::decode(public_key_document)
        .map_err(|error| format!("Invalid QoreDB updater public key: {error}"))?;
    let signature = Signature::decode(&signature_text)
        .map_err(|error| format!("Invalid AI manifest signature format: {error}"))?;
    public_key
        .verify(manifest, &signature, true)
        .map_err(|error| format!("AI manifest signature verification failed: {error}"))
}

fn decode_signature_document(signature: &[u8]) -> Result<String, String> {
    let text = std::str::from_utf8(signature)
        .map_err(|_| "AI manifest signature is not UTF-8".to_string())?
        .trim();
    if text.starts_with("untrusted comment:") {
        return Ok(text.to_string());
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|_| "AI manifest signature is neither Minisign nor base64".to_string())?;
    String::from_utf8(decoded).map_err(|_| "Decoded AI manifest signature is not UTF-8".to_string())
}

fn updater_public_key_document() -> Result<String, String> {
    let config: serde_json::Value = serde_json::from_str(include_str!("../../tauri.conf.json"))
        .map_err(|error| format!("Could not read the updater trust root: {error}"))?;
    let encoded = config
        .pointer("/plugins/updater/pubkey")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "QoreDB updater public key is missing".to_string())?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "QoreDB updater public key is not valid base64".to_string())?;
    String::from_utf8(decoded)
        .map_err(|_| "QoreDB updater public key is not valid UTF-8".to_string())
}

fn parse_and_validate_catalog(
    bytes: &[u8],
    require_embedded_layout: bool,
) -> Result<LocalManifestCatalog, String> {
    let catalog: LocalManifestCatalog = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid Qore AI manifest JSON: {error}"))?;
    validate_catalog(&catalog, require_embedded_layout)?;
    Ok(catalog)
}

fn validate_catalog(
    catalog: &LocalManifestCatalog,
    require_embedded_layout: bool,
) -> Result<(), String> {
    if catalog.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported Qore AI manifest schema {}",
            catalog.schema_version
        ));
    }
    parse_manifest_version(&catalog.version)?;
    validate_artifact(&catalog.model)?;
    if catalog.model.format != ArchiveFormat::Raw {
        return Err("Qore AI model must use the raw artifact format".to_string());
    }

    let expected: HashSet<_> = SUPPORTED_TARGETS.into_iter().collect();
    let mut actual = HashSet::new();
    for target in &catalog.targets {
        if !actual.insert((target.os.as_str(), target.architecture.as_str())) {
            return Err(format!(
                "Duplicate Qore AI target: {}-{}",
                target.os, target.architecture
            ));
        }
        validate_artifact(&target.runtime)?;
        validate_runtime_path(&target.runtime_relative_path)?;
        let expected_format = if target.os == "windows" {
            ArchiveFormat::Zip
        } else {
            ArchiveFormat::TarGz
        };
        if target.runtime.format != expected_format {
            return Err(format!(
                "Invalid runtime archive format for {}-{}",
                target.os, target.architecture
            ));
        }
    }
    if actual != expected {
        return Err(
            "Qore AI manifest must contain all six supported targets exactly once".to_string(),
        );
    }

    if require_embedded_layout {
        let embedded = embedded_catalog()?;
        for target in &catalog.targets {
            let embedded_target = embedded
                .targets
                .iter()
                .find(|candidate| {
                    candidate.os == target.os && candidate.architecture == target.architecture
                })
                .ok_or_else(|| "Embedded Qore AI target is missing".to_string())?;
            if target.runtime_relative_path != embedded_target.runtime_relative_path {
                return Err(format!(
                    "Remote runtime layout changed for {}-{}",
                    target.os, target.architecture
                ));
            }
        }
    }
    Ok(())
}

fn validate_artifact(artifact: &LocalArtifact) -> Result<(), String> {
    let safe_id = !artifact.id.is_empty()
        && artifact.id.len() <= 128
        && artifact
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if !safe_id {
        return Err("Qore AI artifact id contains unsafe characters".to_string());
    }
    if artifact.version.is_empty() || artifact.version.len() > 128 {
        return Err(format!(
            "Invalid version for Qore AI artifact {}",
            artifact.id
        ));
    }
    let url = reqwest::Url::parse(&artifact.url)
        .map_err(|_| format!("Invalid URL for Qore AI artifact {}", artifact.id))?;
    if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() {
        return Err(format!(
            "Qore AI artifact {} must use an HTTPS URL without credentials",
            artifact.id
        ));
    }
    if artifact.size == 0 || artifact.size > MAX_ARTIFACT_BYTES {
        return Err(format!("Invalid size for Qore AI artifact {}", artifact.id));
    }
    if artifact.sha256.len() != 64
        || !artifact
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "Invalid SHA-256 for Qore AI artifact {}",
            artifact.id
        ));
    }
    if artifact.license.is_empty() || artifact.license.len() > 128 {
        return Err(format!(
            "Invalid license for Qore AI artifact {}",
            artifact.id
        ));
    }
    Ok(())
}

fn validate_runtime_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.contains('\\')
        || Path::new(path)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("Qore AI runtime path must stay relative to its install directory".to_string());
    }
    Ok(())
}

fn parse_manifest_version(version: &str) -> Result<(u32, u8, u8, u32), String> {
    let (date, revision) = version
        .split_once('.')
        .ok_or_else(|| format!("Invalid Qore AI manifest version: {version}"))?;
    let mut parts = date.split('-');
    let year = parts.next().and_then(|value| value.parse().ok());
    let month = parts.next().and_then(|value| value.parse().ok());
    let day = parts.next().and_then(|value| value.parse().ok());
    if parts.next().is_some() {
        return Err(format!("Invalid Qore AI manifest version: {version}"));
    }
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return Err(format!("Invalid Qore AI manifest version: {version}"));
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(format!("Invalid Qore AI manifest version: {version}"));
    }
    let revision = revision
        .parse()
        .map_err(|_| format!("Invalid Qore AI manifest version: {version}"))?;
    Ok((year, month, day, revision))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn every_supported_target_has_a_pinned_manifest() {
        for (os, arch) in SUPPORTED_TARGETS {
            let manifest = manifest_for_target(os, arch).unwrap();
            assert!(manifest.runtime.url.starts_with("https://"));
            assert_eq!(manifest.runtime.sha256.len(), 64);
            assert_eq!(manifest.model.sha256.len(), 64);
            assert!(manifest.runtime.size > 0);
            assert!(manifest.model.size > 0);
        }
    }

    #[test]
    fn unsupported_target_is_rejected() {
        assert!(manifest_for_target("freebsd", "x86_64").is_err());
    }

    #[test]
    fn manifest_versions_are_monotonic() {
        assert_eq!(
            compare_manifest_versions("2026-07-23.2", "2026-07-23.1").unwrap(),
            std::cmp::Ordering::Greater
        );
        assert!(compare_manifest_versions("latest", "2026-07-23.1").is_err());
    }

    #[test]
    fn remote_catalog_cannot_change_runtime_layout() {
        let mut catalog = embedded_catalog().unwrap();
        catalog.targets[0].runtime_relative_path = "../llama-server".to_string();
        let bytes = serde_json::to_vec(&catalog).unwrap();
        assert!(parse_and_validate_catalog(&bytes, true).is_err());
    }

    #[tokio::test]
    async fn signed_remote_catalog_is_accepted_byte_exactly() {
        let server = MockServer::start().await;
        let mut catalog = embedded_catalog().unwrap();
        catalog.version = "2026-07-23.1".to_string();
        let manifest = serde_json::to_vec(&catalog).unwrap();
        let (public_key, signature) = sign_test_manifest(&manifest);
        Mock::given(method("GET"))
            .and(path("/manifest.json"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(manifest))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/manifest.json.sig"))
            .respond_with(ResponseTemplate::new(200).set_body_string(signature))
            .expect(1)
            .mount(&server)
            .await;

        let fetched = fetch_signed_catalog(
            &reqwest::Client::new(),
            &format!("{}/manifest.json", server.uri()),
            &public_key,
        )
        .await
        .unwrap();
        assert_eq!(fetched.version, "2026-07-23.1");
        assert_eq!(fetched.targets.len(), SUPPORTED_TARGETS.len());
    }

    #[test]
    fn tampered_signed_manifest_is_rejected() {
        let manifest = EMBEDDED_MANIFEST.to_vec();
        let (public_key, signature) = sign_test_manifest(&manifest);
        let mut tampered = manifest;
        let byte = tampered
            .iter_mut()
            .find(|byte| **byte == b'Q')
            .expect("embedded manifest should contain Q");
        *byte = b'X';

        assert!(verify_minisign(&tampered, signature.as_bytes(), &public_key).is_err());
    }

    #[test]
    fn updater_trust_root_is_a_valid_minisign_key() {
        let document = updater_public_key_document().unwrap();
        PublicKey::decode(&document).unwrap();
    }

    fn sign_test_manifest(manifest: &[u8]) -> (String, String) {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let key_id = [1_u8, 2, 3, 4, 5, 6, 7, 8];
        let mut public_key = Vec::from(*b"Ed");
        public_key.extend_from_slice(&key_id);
        public_key.extend_from_slice(&signing_key.verifying_key().to_bytes());
        let public_document = format!(
            "untrusted comment: qore ai test key\n{}",
            base64::engine::general_purpose::STANDARD.encode(public_key)
        );

        let signature = signing_key.sign(manifest).to_bytes();
        let trusted_comment = "timestamp:0\tfile:qore-ai-local-manifest-v1.json";
        let mut global_input = Vec::from(signature);
        global_input.extend_from_slice(trusted_comment.as_bytes());
        let global_signature = signing_key.sign(&global_input).to_bytes();

        let mut signature_record = Vec::from(*b"Ed");
        signature_record.extend_from_slice(&key_id);
        signature_record.extend_from_slice(&signature);
        let signature_document = format!(
            "untrusted comment: qore ai test signature\n{}\ntrusted comment: {}\n{}",
            base64::engine::general_purpose::STANDARD.encode(signature_record),
            trusted_comment,
            base64::engine::general_purpose::STANDARD.encode(global_signature)
        );
        (public_document, signature_document)
    }
}
