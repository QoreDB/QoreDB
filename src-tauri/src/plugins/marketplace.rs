// SPDX-License-Identifier: Apache-2.0

//! Marketplace install path: fetch a plugin archive, verify its sha256
//! against the catalog-declared digest, extract it into a staging folder,
//! then hand the folder to the existing `install_plugin` flow.
//!
//! The bytes are verified *before* the archive is touched in any way. A
//! hostile mirror or a corrupted download fails fast, with no partial
//! extraction and no manifest parsed against tainted bytes.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::{install_plugin, InstalledPlugin};

/// Caps every marketplace download against. They mirror the host's
/// install-time budget so the existing `install_plugin` would refuse
/// anything bigger anyway — we just fail earlier and clearer.
const MAX_ARCHIVE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ARCHIVE_FILES: usize = 256;
const HTTP_TIMEOUT_SECS: u64 = 30;

/// Hard cap on the catalog index payload — a few hundred plugins fit
/// comfortably; anything bigger is almost certainly a misconfigured mirror.
const MAX_INDEX_BYTES: u64 = 2 * 1024 * 1024;

/// Fetches the marketplace catalog index as opaque JSON. The webview's CSP
/// blocks direct cross-origin fetches, so the index has to come through Rust.
/// The response shape is validated by the frontend against
/// `MarketplaceIndex`; this entry point only enforces transport-level
/// constraints (https, size cap, timeout, JSON parse).
pub fn fetch_index(url: &str) -> Result<serde_json::Value, String> {
    if !url.starts_with("https://") {
        return Err("Marketplace index URL must use https://".into());
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .user_agent(concat!("QoreDB/", env!("CARGO_PKG_VERSION"), " marketplace"))
        .build()
        .map_err(|e| format!("HTTP client init failed: {e}"))?;

    let response = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .map_err(|e| format!("Could not reach the marketplace: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Marketplace responded with HTTP {}",
            response.status()
        ));
    }

    if let Some(content_length) = response.content_length() {
        if content_length > MAX_INDEX_BYTES {
            return Err(format!(
                "Marketplace index exceeds the size limit ({} MiB).",
                MAX_INDEX_BYTES / 1024 / 1024
            ));
        }
    }

    let mut reader = response.take(MAX_INDEX_BYTES + 1);
    let mut bytes = Vec::with_capacity(32 * 1024);
    reader
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Could not read marketplace response: {e}"))?;
    if bytes.len() as u64 > MAX_INDEX_BYTES {
        return Err(format!(
            "Marketplace index exceeds the size limit ({} MiB).",
            MAX_INDEX_BYTES / 1024 / 1024
        ));
    }

    serde_json::from_slice::<serde_json::Value>(&bytes)
        .map_err(|e| format!("Marketplace returned invalid JSON: {e}"))
}

/// Public entry point: download, verify, extract, install.
pub fn install_from_archive_url(
    plugins_dir: &Path,
    url: &str,
    expected_sha256: &str,
) -> Result<InstalledPlugin, String> {
    let expected = normalise_digest(expected_sha256)?;

    let bytes = download_archive(url)?;
    verify_sha256(&bytes, &expected)?;

    let staging = staging_dir(plugins_dir)?;
    // Guard the staging dir against early returns from this point on — every
    // exit path must clean it up so a half-extracted archive isn't picked up
    // by `list_plugins` on the next reload.
    let result = (|| {
        extract_archive(&bytes, &staging)?;
        if !staging.join("plugin.json").exists() {
            return Err(
                "The downloaded archive does not contain a plugin.json at its root.".to_string(),
            );
        }
        let source = staging
            .to_str()
            .ok_or_else(|| "Plugin staging path is not valid UTF-8".to_string())?
            .to_string();
        install_plugin(plugins_dir, &source)
    })();

    // Best-effort cleanup; an install_plugin success has already copied the
    // bytes out of the staging folder, and a failure should leave nothing
    // behind that pollutes the plugin directory.
    let _ = fs::remove_dir_all(&staging);

    result
}

fn normalise_digest(raw: &str) -> Result<[u8; 32], String> {
    let hex = raw.strip_prefix("sha256-").unwrap_or(raw).trim();
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(
            "Expected sha256 digest as 64 lowercase hex characters or `sha256-<64 hex>`.".into(),
        );
    }
    let lower = hex.to_ascii_lowercase();
    let mut out = [0u8; 32];
    for (i, chunk) in lower.as_bytes().chunks(2).enumerate() {
        let pair = std::str::from_utf8(chunk).map_err(|e| e.to_string())?;
        out[i] = u8::from_str_radix(pair, 16).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

fn download_archive(url: &str) -> Result<Vec<u8>, String> {
    // The marketplace serves archives from `raw.githubusercontent.com`. The
    // host doesn't enforce a hostname allow-list — the sha256 check below is
    // what makes the source untrusted-by-default safe.
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .user_agent(concat!("QoreDB/", env!("CARGO_PKG_VERSION"), " marketplace"))
        .build()
        .map_err(|e| format!("HTTP client init failed: {e}"))?;

    let response = client
        .get(url)
        .send()
        .map_err(|e| format!("Could not download the plugin archive: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Plugin archive responded with HTTP {}",
            response.status()
        ));
    }

    if let Some(content_length) = response.content_length() {
        if content_length > MAX_ARCHIVE_BYTES {
            return Err(format!(
                "Plugin archive exceeds the size limit ({} MiB).",
                MAX_ARCHIVE_BYTES / 1024 / 1024
            ));
        }
    }

    // Read with a tight upper bound so a `Content-Length`-less response
    // can't drain memory by streaming a huge archive.
    let mut reader = response.take(MAX_ARCHIVE_BYTES + 1);
    let mut bytes = Vec::with_capacity(64 * 1024);
    reader
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Could not read the plugin archive: {e}"))?;
    if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err(format!(
            "Plugin archive exceeds the size limit ({} MiB).",
            MAX_ARCHIVE_BYTES / 1024 / 1024
        ));
    }
    Ok(bytes)
}

fn verify_sha256(bytes: &[u8], expected: &[u8; 32]) -> Result<(), String> {
    let actual = Sha256::digest(bytes);
    if actual.as_slice() != expected.as_slice() {
        return Err(
            "Archive sha256 mismatch — the downloaded bytes do not match the registry's recorded digest. \
             The host refused to extract it."
                .into(),
        );
    }
    Ok(())
}

fn staging_dir(plugins_dir: &Path) -> Result<PathBuf, String> {
    let parent = plugins_dir
        .parent()
        .ok_or_else(|| "Plugins directory has no parent".to_string())?;
    let staging = parent.join(".marketplace-staging");
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .map_err(|e| format!("Failed to clean marketplace staging folder: {e}"))?;
    }
    fs::create_dir_all(&staging)
        .map_err(|e| format!("Failed to create marketplace staging folder: {e}"))?;
    Ok(staging)
}

fn extract_archive(bytes: &[u8], staging: &Path) -> Result<(), String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| format!("Archive is not a valid zip file: {e}"))?;

    if archive.len() > MAX_ARCHIVE_FILES {
        return Err(format!(
            "Archive has too many files (max {MAX_ARCHIVE_FILES})."
        ));
    }

    let mut total_bytes: u64 = 0;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Could not read archive entry: {e}"))?;

        // Reject anything that isn't a flat file at the archive root.
        let name = entry
            .enclosed_name()
            .ok_or_else(|| format!("Archive entry {} has an unsafe path", entry.name()))?;
        if name.components().count() != 1 {
            return Err(format!(
                "Archive entry {} is not at the root — marketplace archives must be flat",
                entry.name()
            ));
        }
        if entry.is_dir() {
            continue;
        }

        total_bytes = total_bytes.saturating_add(entry.size());
        if total_bytes > MAX_ARCHIVE_BYTES {
            return Err(format!(
                "Archive uncompressed size exceeds the limit ({} MiB).",
                MAX_ARCHIVE_BYTES / 1024 / 1024
            ));
        }

        let dest = staging.join(&name);
        let mut out =
            fs::File::create(&dest).map_err(|e| format!("Could not write {name:?}: {e}"))?;
        std::io::copy(&mut entry, &mut out)
            .map_err(|e| format!("Could not extract {name:?}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_both_digest_shapes() {
        let raw = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let with_prefix = format!("sha256-{raw}");
        let a = normalise_digest(raw).unwrap();
        let b = normalise_digest(&with_prefix).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn rejects_bad_digest_shape() {
        assert!(normalise_digest("not-hex").is_err());
        assert!(normalise_digest("sha256-deadbeef").is_err()); // too short
    }

    #[test]
    fn rejects_mismatched_sha256() {
        let expected = [0u8; 32];
        let err = verify_sha256(b"hello", &expected).unwrap_err();
        assert!(err.contains("mismatch"));
    }
}
