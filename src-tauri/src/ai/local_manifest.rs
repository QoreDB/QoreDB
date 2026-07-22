// SPDX-License-Identifier: BUSL-1.1

//! Pinned, application-owned manifest for Qore AI Local artifacts.
//!
//! URLs never come from the frontend. Updating an artifact requires a source
//! change so its immutable URL, exact size and SHA-256 are reviewed together.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Raw,
    TarGz,
    Zip,
}

#[derive(Debug, Clone)]
pub struct LocalArtifact {
    pub id: &'static str,
    pub version: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
    pub format: ArchiveFormat,
    pub license: &'static str,
}

#[derive(Debug, Clone)]
pub struct LocalArtifactManifest {
    pub version: &'static str,
    pub runtime: LocalArtifact,
    pub runtime_relative_path: &'static str,
    pub model: LocalArtifact,
}

const MODEL: LocalArtifact = LocalArtifact {
    id: "qwen3-8b-q4-k-m",
    version: "212c964b8f97cb5edc203d411b767aaae707e653",
    url: "https://huggingface.co/Qwen/Qwen3-8B-GGUF/resolve/212c964b8f97cb5edc203d411b767aaae707e653/Qwen3-8B-Q4_K_M.gguf?download=true",
    size: 5_027_783_488,
    sha256: "d98cdcbd03e17ce47681435b5150e34c1417f50b5c0019dd560e4882c5745785",
    format: ArchiveFormat::Raw,
    license: "Apache-2.0",
};

pub fn manifest_for_target(os: &str, arch: &str) -> Result<LocalArtifactManifest, String> {
    let (runtime, runtime_relative_path) = match (os, arch) {
        ("macos", "aarch64") => (
            artifact(
                "llama-b10087-macos-arm64",
                "https://github.com/ggml-org/llama.cpp/releases/download/b10087/llama-b10087-bin-macos-arm64.tar.gz",
                10_617_230,
                "1a9ec1c078eb4ff69834e43d7139fd0f13bd03ca3a3e281a9149fc298eda3e86",
                ArchiveFormat::TarGz,
            ),
            "llama-b10087/llama-server",
        ),
        ("macos", "x86_64") => (
            artifact(
                "llama-b10087-macos-x64",
                "https://github.com/ggml-org/llama.cpp/releases/download/b10087/llama-b10087-bin-macos-x64.tar.gz",
                10_888_087,
                "ef36bb008bf7da5f2da6fec6b45034bbac89318193b62bc5e617388e64003fb5",
                ArchiveFormat::TarGz,
            ),
            "llama-b10087/llama-server",
        ),
        ("linux", "aarch64") => (
            artifact(
                "llama-b10087-ubuntu-arm64",
                "https://github.com/ggml-org/llama.cpp/releases/download/b10087/llama-b10087-bin-ubuntu-arm64.tar.gz",
                12_984_024,
                "8c58a1f965013563260c57cef1802f5c42076ea4dad145eaad0e98b084c505ed",
                ArchiveFormat::TarGz,
            ),
            "llama-b10087/llama-server",
        ),
        ("linux", "x86_64") => (
            artifact(
                "llama-b10087-ubuntu-x64",
                "https://github.com/ggml-org/llama.cpp/releases/download/b10087/llama-b10087-bin-ubuntu-x64.tar.gz",
                16_081_073,
                "d7b3da8847ecb776cd2c905589d9a41ffcf37361ff080719020da1d3498b5a75",
                ArchiveFormat::TarGz,
            ),
            "llama-b10087/llama-server",
        ),
        ("windows", "aarch64") => (
            artifact(
                "llama-b10087-windows-arm64",
                "https://github.com/ggml-org/llama.cpp/releases/download/b10087/llama-b10087-bin-win-cpu-arm64.zip",
                11_869_619,
                "2912510b14fe3637df8352ad9772bf53ed87c7ebe270ad1f24067d67df89cc7a",
                ArchiveFormat::Zip,
            ),
            "llama-server.exe",
        ),
        ("windows", "x86_64") => (
            artifact(
                "llama-b10087-windows-x64",
                "https://github.com/ggml-org/llama.cpp/releases/download/b10087/llama-b10087-bin-win-cpu-x64.zip",
                18_022_611,
                "d27d4c8e939cf16d678f3a2e7a6e9e9538c4c42aca960fa804d8c9d35dfaa962",
                ArchiveFormat::Zip,
            ),
            "llama-server.exe",
        ),
        _ => return Err(format!("Unsupported Qore AI Local target: {os}-{arch}")),
    };

    Ok(LocalArtifactManifest {
        version: "2026-07-22.1",
        runtime,
        runtime_relative_path,
        model: MODEL,
    })
}

fn artifact(
    id: &'static str,
    url: &'static str,
    size: u64,
    sha256: &'static str,
    format: ArchiveFormat,
) -> LocalArtifact {
    LocalArtifact {
        id,
        version: "b10087",
        url,
        size,
        sha256,
        format,
        license: "MIT",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_target_has_a_pinned_manifest() {
        for os in ["macos", "windows", "linux"] {
            for arch in ["aarch64", "x86_64"] {
                let manifest = manifest_for_target(os, arch).unwrap();
                assert!(manifest.runtime.url.starts_with("https://"));
                assert_eq!(manifest.runtime.sha256.len(), 64);
                assert_eq!(manifest.model.sha256.len(), 64);
                assert!(manifest.runtime.size > 0);
                assert!(manifest.model.size > 0);
            }
        }
    }

    #[test]
    fn unsupported_target_is_rejected() {
        assert!(manifest_for_target("freebsd", "x86_64").is_err());
    }
}
