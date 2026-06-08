// SPDX-License-Identifier: Apache-2.0

//! Plugin registry: discovery, install, enable/disable and removal.
//!
//! Plugins live in `<dir>/<plugin-id>/plugin.json`. Enabled state is persisted
//! in `<dir>/index.json` as a map of plugin id → bool; a plugin missing from
//! the map is enabled by default.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::manifest;
use super::{InstalledPlugin, PluginContributions};

type EnabledIndex = BTreeMap<String, bool>;

fn read_index(dir: &Path) -> EnabledIndex {
    fs::read_to_string(dir.join("index.json"))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write_index(dir: &Path, index: &EnabledIndex) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(index).map_err(|e| e.to_string())?;
    crate::paths::atomic_write(&dir.join("index.json"), raw.as_bytes())
        .map_err(|e| format!("Failed to write plugin index: {e}"))
}

/// Reserved suffixes [`install_plugin`] uses while swapping versions.
/// [`list_plugins`] filters folders ending with these so an in-flight or
/// crashed install never surfaces as a real plugin.
const STAGING_SUFFIX: &str = ".qoredb-staging";
const BACKUP_SUFFIX: &str = ".qoredb-backup";

/// Lists every installed plugin with its runtime state. Invalid plugin
/// folders are skipped silently — install-time validation is what surfaces
/// errors to the user.
pub fn list_plugins(dir: &Path) -> Vec<InstalledPlugin> {
    let index = read_index(dir);
    let mut plugins = Vec::new();

    let Ok(entries) = fs::read_dir(dir) else {
        return plugins;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.ends_with(STAGING_SUFFIX) || name.ends_with(BACKUP_SUFFIX) {
                continue;
            }
        }
        let Ok(raw) = fs::read_to_string(path.join("plugin.json")) else {
            continue;
        };
        let manifest = match manifest::parse_manifest(&raw) {
            Ok(manifest) => manifest,
            Err(e) => {
                tracing::warn!(plugin = %path.display(), error = %e, "skipping invalid plugin");
                continue;
            }
        };
        let enabled = index.get(&manifest.id).copied().unwrap_or(true);
        let compatible = manifest::is_compatible(manifest.qoredb.as_deref());
        plugins.push(InstalledPlugin {
            dir_name: entry.file_name().to_string_lossy().to_string(),
            enabled,
            compatible,
            manifest,
        });
    }
    plugins.sort_by(|a, b| {
        a.manifest
            .name
            .to_lowercase()
            .cmp(&b.manifest.name.to_lowercase())
    });
    plugins
}

/// Installs (or updates) a plugin from a source folder. The swap goes
/// through a staging folder + atomic rename, with a backup of the previous
/// version restored on failure — the live plugin is never lost mid-install.
pub fn install_plugin(dir: &Path, source: &str) -> Result<InstalledPlugin, String> {
    let source = Path::new(source);
    let raw = fs::read_to_string(source.join("plugin.json"))
        .map_err(|_| "No plugin.json found in the selected folder".to_string())?;
    let manifest = manifest::parse_manifest(&raw)?;

    let target = dir.join(&manifest.id);
    let staging = dir.join(format!("{}{STAGING_SUFFIX}", manifest.id));
    let backup = dir.join(format!("{}{BACKUP_SUFFIX}", manifest.id));

    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| format!("Failed to clean staging folder: {e}"))?;
    }
    if backup.exists() {
        fs::remove_dir_all(&backup).map_err(|e| format!("Failed to clean backup folder: {e}"))?;
    }

    if let Err(e) = copy_dir(source, &staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    let had_previous = target.exists();
    if had_previous {
        if let Err(e) = fs::rename(&target, &backup) {
            let _ = fs::remove_dir_all(&staging);
            return Err(format!("Failed to back up the previous plugin: {e}"));
        }
    }
    if let Err(e) = fs::rename(&staging, &target) {
        let _ = fs::remove_dir_all(&staging);
        if had_previous {
            if let Err(restore_err) = fs::rename(&backup, &target) {
                tracing::error!(
                    plugin = %manifest.id,
                    error = %restore_err,
                    "failed to restore previous plugin after a botched install"
                );
            }
        }
        return Err(format!("Failed to activate the new plugin: {e}"));
    }
    if had_previous {
        let _ = fs::remove_dir_all(&backup);
    }

    let enabled = read_index(dir).get(&manifest.id).copied().unwrap_or(true);
    let compatible = manifest::is_compatible(manifest.qoredb.as_deref());
    Ok(InstalledPlugin {
        dir_name: manifest.id.clone(),
        enabled,
        compatible,
        manifest,
    })
}

/// Removes an installed plugin and its enabled-state entry.
pub fn remove_plugin(dir: &Path, plugin_id: &str) -> Result<(), String> {
    let plugin = find_plugin(dir, plugin_id)?;
    let path = dir.join(&plugin.dir_name);
    if path.exists() {
        fs::remove_dir_all(&path).map_err(|e| format!("Failed to remove plugin: {e}"))?;
    }
    let mut index = read_index(dir);
    index.remove(plugin_id);
    write_index(dir, &index)
}

/// Enables or disables a plugin.
pub fn set_plugin_enabled(dir: &Path, plugin_id: &str, enabled: bool) -> Result<(), String> {
    find_plugin(dir, plugin_id)?;
    let mut index = read_index(dir);
    index.insert(plugin_id.to_string(), enabled);
    write_index(dir, &index)
}

/// Aggregates the contributions of every enabled, compatible plugin. Every
/// contributed id is namespaced by plugin id to avoid collisions.
pub fn get_contributions(dir: &Path) -> PluginContributions {
    let mut merged = PluginContributions::default();
    for plugin in list_plugins(dir) {
        if !plugin.enabled || !plugin.compatible {
            continue;
        }
        let pid = &plugin.manifest.id;
        for mut s in plugin.manifest.contributes.snippets {
            s.id = format!("{pid}::{}", s.id);
            merged.snippets.push(s);
        }
        for mut t in plugin.manifest.contributes.connection_templates {
            t.id = format!("{pid}::{}", t.id);
            merged.connection_templates.push(t);
        }
        for mut th in plugin.manifest.contributes.themes {
            th.id = format!("{pid}::{}", th.id);
            merged.themes.push(th);
        }
        for mut v in plugin.manifest.contributes.result_viewers {
            v.id = format!("{pid}::{}", v.id);
            merged.result_viewers.push(v);
        }
        for mut c in plugin.manifest.contributes.commands {
            c.id = format!("{pid}::{}", c.id);
            merged.commands.push(c);
        }
    }
    merged
}

fn find_plugin(dir: &Path, plugin_id: &str) -> Result<InstalledPlugin, String> {
    list_plugins(dir)
        .into_iter()
        .find(|p| p.manifest.id == plugin_id)
        .ok_or_else(|| format!("Plugin '{plugin_id}' is not installed"))
}

/// Caps preventing a runaway or malicious source folder from filling the
/// disk or recursing through a symlink.
const MAX_PLUGIN_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PLUGIN_FILES: usize = 256;

struct CopyBudget {
    bytes: u64,
    files: usize,
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    let mut budget = CopyBudget {
        bytes: MAX_PLUGIN_BYTES,
        files: MAX_PLUGIN_FILES,
    };
    copy_dir_bounded(from, to, &mut budget)
}

/// Directory and file names skipped during install: build artefacts and VCS
/// metadata that are never useful at runtime and would otherwise blow past
/// the install budget on a freshly built source folder.
fn is_skipped_entry(name: &str) -> bool {
    matches!(
        name,
        "target" | "node_modules" | ".git" | ".cargo" | ".DS_Store" | "Cargo.lock"
    )
}

fn copy_dir_bounded(from: &Path, to: &Path, budget: &mut CopyBudget) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("Failed to create plugin directory: {e}"))?;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if is_skipped_entry(name) {
                continue;
            }
        }
        // symlink_metadata, not metadata: a symlink to an ancestor would
        // otherwise recurse forever.
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            return Err(format!(
                "Plugin folder contains a symbolic link ('{}'), which is not allowed",
                entry.file_name().to_string_lossy()
            ));
        }
        let dest = to.join(entry.file_name());
        if meta.is_dir() {
            copy_dir_bounded(&path, &dest, budget)?;
        } else {
            budget.files = budget
                .files
                .checked_sub(1)
                .ok_or_else(|| format!("Plugin has too many files (limit {MAX_PLUGIN_FILES})"))?;
            budget.bytes = budget.bytes.checked_sub(meta.len()).ok_or_else(|| {
                format!(
                    "Plugin exceeds the size limit ({} MiB)",
                    MAX_PLUGIN_BYTES / 1024 / 1024
                )
            })?;
            fs::copy(&path, &dest).map_err(|e| format!("Failed to copy plugin file: {e}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "qoredb_plugins_test_{}_{}",
            tag,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&base).unwrap();
        base
    }

    fn write_plugin(dir: &Path, id: &str, manifest_json: &str) {
        let plugin_dir = dir.join(id);
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(plugin_dir.join("plugin.json"), manifest_json).unwrap();
    }

    const SAMPLE: &str = r#"{
        "id":"acme.pack","name":"Pack","version":"1.0.0",
        "contributes":{"snippets":[{"id":"hello","label":"Hello","template":"SELECT 1;"}]}
    }"#;

    #[test]
    fn lists_installed_plugins_enabled_by_default() {
        let dir = temp_dir("list");
        write_plugin(&dir, "acme.pack", SAMPLE);
        let plugins = list_plugins(&dir);
        assert_eq!(plugins.len(), 1);
        assert!(plugins[0].enabled);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn skips_invalid_plugin_folders() {
        let dir = temp_dir("invalid");
        write_plugin(&dir, "bad", r#"{"id":"BAD ID","name":"x","version":"1"}"#);
        assert!(list_plugins(&dir).is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn disabling_a_plugin_drops_its_contributions() {
        let dir = temp_dir("disable");
        write_plugin(&dir, "acme.pack", SAMPLE);
        assert_eq!(get_contributions(&dir).snippets.len(), 1);

        set_plugin_enabled(&dir, "acme.pack", false).unwrap();
        assert!(!list_plugins(&dir)[0].enabled);
        assert!(get_contributions(&dir).snippets.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn contributions_are_namespaced_by_plugin_id() {
        let dir = temp_dir("namespace");
        write_plugin(&dir, "acme.pack", SAMPLE);
        let contributions = get_contributions(&dir);
        assert_eq!(contributions.snippets[0].id, "acme.pack::hello");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn removing_a_plugin_deletes_it() {
        let dir = temp_dir("remove");
        write_plugin(&dir, "acme.pack", SAMPLE);
        remove_plugin(&dir, "acme.pack").unwrap();
        assert!(list_plugins(&dir).is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    /// Builds a source plugin folder a caller can hand to [`install_plugin`].
    fn write_source_plugin(parent: &Path, name: &str, manifest_json: &str) -> PathBuf {
        let source = parent.join(name);
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("plugin.json"), manifest_json).unwrap();
        source
    }

    #[test]
    fn install_creates_the_plugin_folder() {
        let dir = temp_dir("install_fresh");
        let source = write_source_plugin(&dir, "src", SAMPLE);

        let installed = install_plugin(&dir, source.to_str().unwrap()).unwrap();
        assert_eq!(installed.manifest.id, "acme.pack");
        assert!(dir.join("acme.pack").is_dir());
        assert!(!dir.join(format!("acme.pack{STAGING_SUFFIX}")).exists());
        assert!(!dir.join(format!("acme.pack{BACKUP_SUFFIX}")).exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn install_skips_build_artefact_directories() {
        let dir = temp_dir("install_skip_artefacts");
        let source = dir.join("src");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("plugin.json"), SAMPLE).unwrap();
        // Drop bulky artefact folders + lockfile that a `cargo build` would
        // leave behind. None of them must end up in the installed copy.
        let target = source.join("target");
        fs::create_dir_all(target.join("release")).unwrap();
        fs::write(
            target.join("release").join("huge.bin"),
            vec![0u8; 16 * 1024 * 1024],
        )
        .unwrap();
        fs::create_dir_all(source.join(".git")).unwrap();
        fs::write(source.join(".git").join("HEAD"), b"ref: refs/heads/main").unwrap();
        fs::write(source.join("Cargo.lock"), b"# lock").unwrap();

        install_plugin(&dir, source.to_str().unwrap()).unwrap();
        let installed = dir.join("acme.pack");
        assert!(installed.join("plugin.json").exists());
        assert!(
            !installed.join("target").exists(),
            "target/ must be skipped"
        );
        assert!(!installed.join(".git").exists(), ".git/ must be skipped");
        assert!(
            !installed.join("Cargo.lock").exists(),
            "Cargo.lock must be skipped"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn install_overwrites_an_existing_plugin() {
        let dir = temp_dir("install_replace");
        let installed_dir = dir.join("acme.pack");
        fs::create_dir_all(&installed_dir).unwrap();
        fs::write(installed_dir.join("plugin.json"), SAMPLE).unwrap();
        fs::write(installed_dir.join("OLD"), b"v1").unwrap();

        let source = write_source_plugin(&dir, "src", SAMPLE);

        install_plugin(&dir, source.to_str().unwrap()).unwrap();
        assert!(!dir.join("acme.pack").join("OLD").exists());
        assert!(!dir.join(format!("acme.pack{BACKUP_SUFFIX}")).exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn install_failure_leaves_previous_plugin_in_place() {
        let dir = temp_dir("install_rollback");
        let installed_dir = dir.join("acme.pack");
        fs::create_dir_all(&installed_dir).unwrap();
        fs::write(installed_dir.join("plugin.json"), SAMPLE).unwrap();
        fs::write(installed_dir.join("MARKER"), b"survivor").unwrap();

        // Source that breaches the file budget so copy_dir fails before any
        // rename happens.
        let source = dir.join("src");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("plugin.json"), SAMPLE).unwrap();
        for i in 0..(MAX_PLUGIN_FILES + 2) {
            fs::write(source.join(format!("f{i}.txt")), b"x").unwrap();
        }

        let err = install_plugin(&dir, source.to_str().unwrap())
            .expect_err("install should fail when the source breaks the budget");
        assert!(err.contains("too many files"), "unexpected error: {err}");

        assert!(installed_dir.join("MARKER").exists());
        assert!(!dir.join(format!("acme.pack{STAGING_SUFFIX}")).exists());
        assert!(!dir.join(format!("acme.pack{BACKUP_SUFFIX}")).exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn install_cleans_stale_staging_and_backup_folders() {
        let dir = temp_dir("install_cleanup");
        let staging = dir.join(format!("acme.pack{STAGING_SUFFIX}"));
        let backup = dir.join(format!("acme.pack{BACKUP_SUFFIX}"));
        fs::create_dir_all(&staging).unwrap();
        fs::create_dir_all(&backup).unwrap();
        fs::write(staging.join("leftover"), b"x").unwrap();

        let source = write_source_plugin(&dir, "src", SAMPLE);
        install_plugin(&dir, source.to_str().unwrap()).unwrap();

        assert!(dir.join("acme.pack").is_dir());
        assert!(!staging.exists());
        assert!(!backup.exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn list_plugins_skips_staging_and_backup_folders() {
        let dir = temp_dir("list_skip");
        write_plugin(&dir, "acme.pack", SAMPLE);
        let staging = dir.join(format!("acme.pack{STAGING_SUFFIX}"));
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("plugin.json"), SAMPLE).unwrap();
        let backup = dir.join(format!("acme.pack{BACKUP_SUFFIX}"));
        fs::create_dir_all(&backup).unwrap();
        fs::write(backup.join("plugin.json"), SAMPLE).unwrap();

        let plugins = list_plugins(&dir);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].dir_name, "acme.pack");
        fs::remove_dir_all(&dir).ok();
    }
}
