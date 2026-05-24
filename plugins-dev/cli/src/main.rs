// SPDX-License-Identifier: Apache-2.0

//! `qoredb-plugin` — companion CLI for writing QoreDB executable plugins.
//!
//! Three subcommands:
//!   * `new <id>`    scaffolds a Cargo crate + plugin.json + lib.rs.
//!   * `build`       runs `cargo build --release --target wasm32-...`,
//!                   copies the `.wasm` next to the manifest, and writes
//!                   the fresh sha256 into `runtime.integrity`.
//!   * `install`     copies the plugin folder into QoreDB's data directory.
//!
//! Kept deliberately small: no async runtime, no fancy progress bars. The
//! tool's only job is to make the manual `cargo build` → `copy` → `install`
//! loop a single command.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use clap::{Parser, Subcommand};
use sha2::{Digest, Sha256};

#[derive(Parser)]
#[command(name = "qoredb-plugin", version, about = "QoreDB plugin scaffolding, build and install.")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Scaffold a new plugin under `<id>/`.
    New {
        /// Reverse-DNS plugin id, e.g. `acme.audit`.
        id: String,
    },
    /// Build the WASM module of the plugin in the current directory and
    /// refresh its sha256 integrity digest.
    Build {
        /// Path to the plugin folder. Defaults to the current directory.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Install the plugin in the current directory into QoreDB's plugins dir.
    Install {
        /// Path to the plugin folder. Defaults to the current directory.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Cmd::New { id } => cmd_new(&id),
        Cmd::Build { path } => cmd_build(&path),
        Cmd::Install { path } => cmd_install(&path),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_new(id: &str) -> Result<(), String> {
    if !is_valid_id(id) {
        return Err(format!(
            "invalid plugin id '{id}': use lowercase letters, digits, '.', '-' or '_'"
        ));
    }
    let dir = PathBuf::from(id);
    if dir.exists() {
        return Err(format!("'{id}' already exists; pick another id or remove it"));
    }
    let crate_name = id.replace('.', "-").replace('_', "-");
    let entry_basename = id.replace(['.', '-'], "_");

    std::fs::create_dir_all(dir.join("src")).map_err(|e| format!("create src/: {e}"))?;
    std::fs::write(
        dir.join("plugin.json"),
        format_manifest(id, &format!("{entry_basename}.wasm")),
    )
    .map_err(|e| format!("write plugin.json: {e}"))?;
    std::fs::write(dir.join("Cargo.toml"), format_cargo_toml(&crate_name))
        .map_err(|e| format!("write Cargo.toml: {e}"))?;
    std::fs::write(dir.join("src").join("lib.rs"), STARTER_LIB_RS)
        .map_err(|e| format!("write src/lib.rs: {e}"))?;
    std::fs::write(dir.join(".gitignore"), "/target\nCargo.lock\n")
        .ok();

    println!("Scaffolded {id} in ./{id}/");
    println!();
    println!("Next steps:");
    println!("  cd {id}");
    println!("  # edit src/lib.rs and plugin.json");
    println!("  qoredb-plugin build");
    println!("  qoredb-plugin install");
    Ok(())
}

fn cmd_build(path: &Path) -> Result<(), String> {
    let path = canonical(path)?;
    let manifest_path = path.join("plugin.json");
    let manifest_raw = std::fs::read_to_string(&manifest_path)
        .map_err(|_| format!("no plugin.json under {}", path.display()))?;
    let mut manifest: serde_json::Value = serde_json::from_str(&manifest_raw)
        .map_err(|e| format!("plugin.json is not valid JSON: {e}"))?;
    let runtime = manifest
        .get("runtime")
        .ok_or_else(|| "plugin.json has no 'runtime' block — only executable plugins can be built".to_string())?;
    let entry = runtime
        .get("entry")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "runtime.entry is missing".to_string())?
        .to_string();

    // Cargo crate sits next to plugin.json. We honour the manifest the user
    // edited, no metadata round-trip needed.
    let cargo_toml = path.join("Cargo.toml");
    if !cargo_toml.exists() {
        return Err(format!("no Cargo.toml under {}", path.display()));
    }
    let crate_name = read_crate_name(&cargo_toml)?;

    println!("=> cargo build --release --target wasm32-unknown-unknown");
    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--target")
        .arg("wasm32-unknown-unknown")
        .current_dir(&path)
        .status()
        .map_err(|e| format!("could not spawn cargo: {e}"))?;
    if !status.success() {
        return Err("cargo build failed".into());
    }

    let normalised = crate_name.replace('-', "_");
    let built = path
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join(format!("{normalised}.wasm"));
    if !built.exists() {
        return Err(format!(
            "expected {} to exist after the build — does Cargo.toml declare [lib] crate-type = [\"cdylib\"]?",
            built.display()
        ));
    }

    let dest = path.join(&entry);
    std::fs::copy(&built, &dest).map_err(|e| format!("copy {} -> {}: {e}", built.display(), dest.display()))?;
    println!("=> wrote {}", dest.display());

    let bytes = std::fs::read(&dest).map_err(|e| format!("read {}: {e}", dest.display()))?;
    let digest = sha256_hex(&bytes);
    let integrity = format!("sha256-{digest}");

    if let Some(runtime_obj) = manifest
        .get_mut("runtime")
        .and_then(|v| v.as_object_mut())
    {
        runtime_obj.insert(
            "integrity".to_string(),
            serde_json::Value::String(integrity.clone()),
        );
    }
    let updated = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("serialise manifest: {e}"))?;
    std::fs::write(&manifest_path, format!("{updated}\n"))
        .map_err(|e| format!("write plugin.json: {e}"))?;
    println!("=> integrity {integrity}");

    Ok(())
}

fn cmd_install(path: &Path) -> Result<(), String> {
    let path = canonical(path)?;
    let manifest_raw = std::fs::read_to_string(path.join("plugin.json"))
        .map_err(|_| format!("no plugin.json under {}", path.display()))?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_raw)
        .map_err(|e| format!("plugin.json is not valid JSON: {e}"))?;
    let id = manifest
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "plugin.json has no 'id'".to_string())?;
    if !is_valid_id(id) {
        return Err(format!("plugin id '{id}' is invalid"));
    }

    let dest_root = plugins_dir();
    std::fs::create_dir_all(&dest_root).map_err(|e| format!("create {}: {e}", dest_root.display()))?;
    let dest = dest_root.join(id);

    // Wipe any previous version. The atomic install lives inside QoreDB
    // itself; here we don't need staging because the CLI is interactive.
    if dest.exists() {
        std::fs::remove_dir_all(&dest)
            .map_err(|e| format!("remove existing {}: {e}", dest.display()))?;
    }
    copy_dir(&path, &dest)?;

    println!("=> installed {id} into {}", dest.display());
    println!("Refresh the Plugins panel in QoreDB to pick up the change.");
    Ok(())
}

fn canonical(path: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Mirrors `crate::paths::app_data_dir` in `src-tauri`. Kept in lockstep
/// manually — the CLI is standalone, so we cannot import that function.
fn plugins_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.qoredb.app")
        .join("plugins")
}

/// Reads `[package].name` from a Cargo.toml. We avoid a `cargo metadata`
/// round-trip — the value sits one toml-parse away.
fn read_crate_name(cargo_toml: &Path) -> Result<String, String> {
    let raw = std::fs::read_to_string(cargo_toml).map_err(|e| format!("read Cargo.toml: {e}"))?;
    let doc: toml_edit::DocumentMut = raw
        .parse()
        .map_err(|e| format!("parse Cargo.toml: {e}"))?;
    let name = doc
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .ok_or_else(|| "Cargo.toml has no [package].name".to_string())?
        .to_string();
    Ok(name)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Loose plugin-id grammar — matches the host's `is_valid_id`.
fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("create {}: {e}", to.display()))?;
    for entry in std::fs::read_dir(from).map_err(|e| format!("read_dir {}: {e}", from.display()))? {
        let entry = entry.map_err(|e| format!("read entry: {e}"))?;
        let src = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Don't ship the build artefacts directory along with the plugin.
        if name_str == "target" || name_str == ".git" {
            continue;
        }
        let dst = to.join(&name);
        let meta = std::fs::symlink_metadata(&src)
            .map_err(|e| format!("stat {}: {e}", src.display()))?;
        if meta.file_type().is_symlink() {
            return Err(format!(
                "refusing to copy symlink {} — QoreDB rejects plugin folders that contain symlinks",
                src.display()
            ));
        }
        if meta.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| format!("copy {}: {e}", src.display()))?;
        }
    }
    Ok(())
}

fn format_manifest(id: &str, entry: &str) -> String {
    format!(
        "{{\n  \"$schema\": \"https://qoredb.com/schemas/plugin.schema.json\",\n  \"id\": \"{id}\",\n  \"name\": \"{id}\",\n  \"version\": \"0.1.0\",\n  \"description\": \"A QoreDB plugin.\",\n  \"qoredb\": \">=0.1.29\",\n  \"runtime\": {{\n    \"abiVersion\": 1,\n    \"entry\": \"{entry}\",\n    \"hooks\": [\"preExecute\"]\n  }}\n}}\n"
    )
}

fn format_cargo_toml(crate_name: &str) -> String {
    format!(
        "[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\nlicense = \"Apache-2.0\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\n# Point this at the SDK location of your QoreDB checkout, or the published\n# crate once one is available.\nqoredb-plugin-sdk = {{ path = \"../sdk\" }}\nserde_json = \"1\"\n\n[profile.release]\nopt-level = \"s\"\nlto = true\nstrip = true\npanic = \"abort\"\n"
    )
}

const STARTER_LIB_RS: &str = r#"// SPDX-License-Identifier: Apache-2.0

//! A QoreDB plugin. Edit `check` to taste; the host calls it once per query.

use qoredb_plugin_sdk::{export_pre_execute, Decision, HookContext};

fn check(_ctx: HookContext) -> Decision {
    Decision::allow()
}

export_pre_execute!(check);
"#;
