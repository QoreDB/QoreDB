// SPDX-License-Identifier: BUSL-1.1

//! On-disk layout for contracts inside a workspace:
//!
//! ```text
//! <workspace>/contracts/
//!   ├── <name>.yml                 ← canonical YAML serialization
//!   └── .history/<name>.jsonl      ← append-only run history (newest at EOF)
//! ```
//!
//! All filenames derive from the canonical contract `name` (validated to
//! match `[A-Za-z_][A-Za-z0-9_]*` by the parser), so no path traversal is
//! ever needed: we never trust a user-supplied path here.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::parser::{extract_name, parse_contract, ContractError, Format};
use super::{Contract, ContractMeta, ContractRun};

const CONTRACTS_DIR: &str = "contracts";
const HISTORY_DIR: &str = ".history";
const YAML_EXT: &str = "yml";
const HISTORY_EXT: &str = "jsonl";
/// Cap on lines kept per contract history file. The newest 200 runs are
/// preserved on rotation — older runs are dropped to keep `get_contract_history`
/// fast and bound disk usage. Adjust here if needed.
const HISTORY_MAX_RUNS: usize = 200;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid contract name {0:?} — must match [A-Za-z_][A-Za-z0-9_]*")]
    InvalidName(String),
    #[error("contract not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Parse(#[from] ContractError),
    #[error("serialization error: {0}")]
    Serialize(String),
}

fn io_err(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> StorageError {
    let path = path.into();
    move |source| StorageError::Io { path, source }
}

/// Returns `<workspace>/contracts/`, creating it on first access. The parent
/// directory must already exist (it's the workspace root).
pub fn contracts_dir(workspace_root: &Path) -> Result<PathBuf, StorageError> {
    let dir = workspace_root.join(CONTRACTS_DIR);
    fs::create_dir_all(&dir).map_err(io_err(dir.clone()))?;
    Ok(dir)
}

fn history_dir(workspace_root: &Path) -> Result<PathBuf, StorageError> {
    let dir = contracts_dir(workspace_root)?.join(HISTORY_DIR);
    fs::create_dir_all(&dir).map_err(io_err(dir.clone()))?;
    Ok(dir)
}

/// Validates a contract name for use as a filename. Mirrors the parser's
/// `ident_regex`. Refuses anything that would let a caller escape the
/// contracts directory or shadow the history subdir.
fn validate_name(name: &str) -> Result<(), StorageError> {
    if name.is_empty() || name.len() > 128 {
        return Err(StorageError::InvalidName(name.to_string()));
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(StorageError::InvalidName(name.to_string()));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(StorageError::InvalidName(name.to_string()));
    }
    Ok(())
}

pub fn contract_path(workspace_root: &Path, name: &str) -> Result<PathBuf, StorageError> {
    validate_name(name)?;
    Ok(contracts_dir(workspace_root)?.join(format!("{name}.{YAML_EXT}")))
}

fn history_path(workspace_root: &Path, name: &str) -> Result<PathBuf, StorageError> {
    validate_name(name)?;
    Ok(history_dir(workspace_root)?.join(format!("{name}.{HISTORY_EXT}")))
}

/// Lists every `<name>.yml` in the contracts dir and parses its name + rule
/// count. Files that fail to parse or whose filename doesn't match the
/// canonical name are skipped silently (best-effort indexing — the editor
/// will surface the error when opened).
pub fn list_contracts(workspace_root: &Path) -> Result<Vec<ContractMeta>, StorageError> {
    let dir = contracts_dir(workspace_root)?;
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(StorageError::Io { path: dir, source: err }),
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !ext.eq_ignore_ascii_case(YAML_EXT) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        // Prefer the embedded name (canonical); fall back to filename stem
        // for files that don't deserialize cleanly so the user can still see
        // them in the list and fix the issue from the editor.
        let name = extract_name(&source).unwrap_or_else(|| stem.to_string());
        let rules_count = parse_contract(&source, Format::Auto)
            .map(|c| c.rules.len() as u32)
            .unwrap_or(0);
        let last_run = read_history(workspace_root, &name, Some(1))
            .ok()
            .and_then(|mut v| v.pop());

        out.push(ContractMeta {
            id: name.clone(),
            name,
            path: path.to_string_lossy().into_owned(),
            rules_count,
            last_run,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Reads the YAML source for a contract by name. Returns the raw bytes so
/// the caller can show the user the on-disk content unmodified (important
/// for the editor round-trip).
pub fn load_contract_source(
    workspace_root: &Path,
    name: &str,
) -> Result<String, StorageError> {
    let path = contract_path(workspace_root, name)?;
    match fs::read_to_string(&path) {
        Ok(s) => Ok(s),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Err(StorageError::NotFound(name.to_string()))
        }
        Err(err) => Err(StorageError::Io { path, source: err }),
    }
}

/// Validates the source then writes it atomically (write to `<name>.yml.tmp`,
/// rename). The embedded `name` field must match the destination filename so
/// `list_contracts` stays consistent.
pub fn save_contract_source(
    workspace_root: &Path,
    name: &str,
    source: &str,
) -> Result<Contract, StorageError> {
    let contract = parse_contract(source, Format::Auto)?;
    if contract.name != name {
        return Err(StorageError::InvalidName(format!(
            "embedded name {:?} differs from filename {:?}",
            contract.name, name
        )));
    }
    let dest = contract_path(workspace_root, name)?;
    let tmp = dest.with_extension(format!("{YAML_EXT}.tmp"));
    {
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .map_err(io_err(tmp.clone()))?;
        f.write_all(source.as_bytes()).map_err(io_err(tmp.clone()))?;
        f.sync_all().map_err(io_err(tmp.clone()))?;
    }
    fs::rename(&tmp, &dest).map_err(io_err(dest.clone()))?;
    Ok(contract)
}

/// Appends a run to `<workspace>/contracts/.history/<name>.jsonl`. Rotates
/// when the file exceeds `HISTORY_MAX_RUNS` lines: rewrites it with the most
/// recent runs only.
pub fn append_run(workspace_root: &Path, name: &str, run: &ContractRun) -> Result<(), StorageError> {
    let path = history_path(workspace_root, name)?;
    let line = serde_json::to_string(run).map_err(|e| StorageError::Serialize(e.to_string()))?;

    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(io_err(path.clone()))?;
    f.write_all(line.as_bytes()).map_err(io_err(path.clone()))?;
    f.write_all(b"\n").map_err(io_err(path.clone()))?;
    drop(f);

    rotate_if_needed(&path)?;
    Ok(())
}

fn rotate_if_needed(path: &Path) -> Result<(), StorageError> {
    let count = {
        let f = match fs::File::open(path) {
            Ok(f) => f,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(StorageError::Io { path: path.into(), source: err }),
        };
        BufReader::new(f).lines().count()
    };
    if count <= HISTORY_MAX_RUNS {
        return Ok(());
    }
    let f = fs::File::open(path).map_err(io_err(path.to_path_buf()))?;
    let lines: Vec<String> = BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .collect();
    let keep_from = lines.len().saturating_sub(HISTORY_MAX_RUNS);
    let tmp = path.with_extension("jsonl.tmp");
    {
        let mut out = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .map_err(io_err(tmp.clone()))?;
        for line in &lines[keep_from..] {
            out.write_all(line.as_bytes()).map_err(io_err(tmp.clone()))?;
            out.write_all(b"\n").map_err(io_err(tmp.clone()))?;
        }
    }
    fs::rename(&tmp, path).map_err(io_err(path.to_path_buf()))?;
    Ok(())
}

/// Reads up to `limit` runs from history, ordered oldest → newest. Caller
/// can pop from the end to get the most recent run.
pub fn read_history(
    workspace_root: &Path,
    name: &str,
    limit: Option<usize>,
) -> Result<Vec<ContractRun>, StorageError> {
    let path = history_path(workspace_root, name)?;
    let f = match fs::File::open(&path) {
        Ok(f) => f,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(StorageError::Io { path, source: err }),
    };

    let all: Vec<ContractRun> = BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<ContractRun>(&l).ok())
        .collect();

    let out = match limit {
        Some(n) if n < all.len() => {
            let skip = all.len() - n;
            all.into_iter().skip(skip).collect()
        }
        _ => all,
    };
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Contract, ContractRun, ContractTarget, Rule, RuleStatus};
    use tempfile::TempDir;

    fn sample_contract(name: &str) -> String {
        format!(
            "name: {name}\nversion: 1\ntarget:\n  connection: c\n  table: t\nrules:\n  - id: r1\n    type: not_empty\n    column: col1\n"
        )
    }

    fn sample_run(contract_name: &str) -> ContractRun {
        ContractRun {
            contract_id: contract_name.to_string(),
            contract_name: contract_name.to_string(),
            connection_id: "c".into(),
            started_at: "2026-05-13T10:00:00Z".into(),
            finished_at: "2026-05-13T10:00:01Z".into(),
            duration_ms: 1000,
            pass_count: 1,
            fail_count: 0,
            error_count: 0,
            results: Vec::new(),
        }
    }

    #[test]
    fn save_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let src = sample_contract("orders_quality");
        save_contract_source(tmp.path(), "orders_quality", &src).unwrap();

        let loaded = load_contract_source(tmp.path(), "orders_quality").unwrap();
        assert_eq!(loaded, src);
    }

    #[test]
    fn save_rejects_mismatched_name() {
        let tmp = TempDir::new().unwrap();
        let src = sample_contract("real_name");
        let err = save_contract_source(tmp.path(), "filename_name", &src).unwrap_err();
        assert!(matches!(err, StorageError::InvalidName(_)));
    }

    #[test]
    fn load_unknown_returns_not_found() {
        let tmp = TempDir::new().unwrap();
        let err = load_contract_source(tmp.path(), "nope").unwrap_err();
        assert!(matches!(err, StorageError::NotFound(_)));
    }

    #[test]
    fn list_contracts_indexes_files() {
        let tmp = TempDir::new().unwrap();
        save_contract_source(tmp.path(), "a", &sample_contract("a")).unwrap();
        save_contract_source(tmp.path(), "b", &sample_contract("b")).unwrap();

        let list = list_contracts(tmp.path()).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "a");
        assert_eq!(list[0].rules_count, 1);
        assert_eq!(list[1].name, "b");
    }

    #[test]
    fn list_contracts_empty_dir_ok() {
        let tmp = TempDir::new().unwrap();
        let list = list_contracts(tmp.path()).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn validate_name_refuses_traversal() {
        assert!(validate_name("../etc").is_err());
        assert!(validate_name("foo/bar").is_err());
        assert!(validate_name("").is_err());
        assert!(validate_name("foo bar").is_err());
        assert!(validate_name("foo.yml").is_err());
        assert!(validate_name("foo_1").is_ok());
        assert!(validate_name("_foo").is_ok());
        assert!(validate_name("1foo").is_err()); // must start with letter or _
    }

    #[test]
    fn history_append_and_read() {
        let tmp = TempDir::new().unwrap();
        save_contract_source(tmp.path(), "orders", &sample_contract("orders")).unwrap();

        for _ in 0..3 {
            append_run(tmp.path(), "orders", &sample_run("orders")).unwrap();
        }
        let runs = read_history(tmp.path(), "orders", None).unwrap();
        assert_eq!(runs.len(), 3);

        let last = read_history(tmp.path(), "orders", Some(1)).unwrap();
        assert_eq!(last.len(), 1);
    }

    #[test]
    fn history_rotates_past_max() {
        let tmp = TempDir::new().unwrap();
        save_contract_source(tmp.path(), "orders", &sample_contract("orders")).unwrap();

        for _ in 0..(HISTORY_MAX_RUNS + 5) {
            append_run(tmp.path(), "orders", &sample_run("orders")).unwrap();
        }
        let runs = read_history(tmp.path(), "orders", None).unwrap();
        assert_eq!(runs.len(), HISTORY_MAX_RUNS);
    }

    #[test]
    fn list_contracts_includes_last_run() {
        let tmp = TempDir::new().unwrap();
        save_contract_source(tmp.path(), "orders", &sample_contract("orders")).unwrap();
        let mut run = sample_run("orders");
        run.pass_count = 7;
        append_run(tmp.path(), "orders", &run).unwrap();

        let list = list_contracts(tmp.path()).unwrap();
        assert_eq!(list.len(), 1);
        let last = list[0].last_run.as_ref().unwrap();
        assert_eq!(last.pass_count, 7);
        let _ = RuleStatus::Pass; // import-keep
        let _ = ContractTarget {
            connection: String::new(),
            schema: None,
            table: String::new(),
        };
        let _: Option<Contract> = None;
        let _: Option<Rule> = None;
    }
}
