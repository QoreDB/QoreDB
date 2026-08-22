// SPDX-License-Identifier: BUSL-1.1

//! Replay set storage in `.qoredb/replays/`.
//!
//! A set is the shareable half of the Replay Lab: queries and expectations,
//! meant to be committed alongside the schema they exercise. **Result rows**
//! never land here — they live under the app data dir (see `capture.rs`).
//!
//! What does land here is the query text, verbatim. A replayable query cannot
//! be redacted, so a literal inside it — `WHERE api_key = '…'`, `AUTH …`,
//! a Redis `SET` payload — is committed with the set. Same trade-off as the
//! query library, which is already versioned with raw SQL, and the UI says so
//! before a recording starts.

use std::fs;
use std::path::{Path, PathBuf};

use super::types::{ReplaySet, ReplaySetSummary};

pub const SET_EXTENSION: &str = ".qreplay.json";
pub const REPLAYS_DIR: &str = "replays";

/// Guards against a set file large enough to stall the UI thread on parse.
const MAX_SET_BYTES: u64 = 16 * 1024 * 1024;

pub struct ReplaySetStore {
    dir: PathBuf,
}

pub fn validate_slug(slug: &str) -> Result<(), String> {
    if slug.is_empty() {
        return Err("Replay set name cannot be empty".to_string());
    }
    if slug.len() > 128 {
        return Err("Replay set name is too long".to_string());
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(
            "Replay set name must only contain alphanumeric characters, underscores, or hyphens"
                .to_string(),
        );
    }
    Ok(())
}

/// Turns a free-form name into a filename stem. Empty input and
/// all-punctuation input both yield an error rather than a silent fallback.
pub fn slugify(name: &str) -> Result<String, String> {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug.truncate(128);
    validate_slug(&slug)?;
    Ok(slug)
}

impl ReplaySetStore {
    pub fn new(workspace_path: &Path) -> Self {
        Self {
            dir: workspace_path.join(REPLAYS_DIR),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file_path(&self, slug: &str) -> Result<PathBuf, String> {
        validate_slug(slug)?;
        Ok(self.dir.join(format!("{}{}", slug, SET_EXTENSION)))
    }

    pub fn path_for(&self, slug: &str) -> Result<PathBuf, String> {
        self.file_path(slug)
    }

    pub fn list(&self) -> Result<Vec<ReplaySetSummary>, String> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("Failed to read replays directory: {}", e)),
        };

        let mut summaries = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(slug) = filename.strip_suffix(SET_EXTENSION) else {
                continue;
            };
            if validate_slug(slug).is_err() {
                continue;
            }
            let Ok(set) = self.load(slug) else { continue };
            summaries.push(ReplaySetSummary {
                slug: slug.to_string(),
                name: set.name,
                created_at: set.created_at,
                driver_id: set.source.driver_id,
                environment: set.source.environment,
                entry_count: set.entries.len(),
                redacted: set.redacted,
            });
        }

        summaries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(summaries)
    }

    pub fn load(&self, slug: &str) -> Result<ReplaySet, String> {
        let path = self.file_path(slug)?;
        let metadata =
            fs::metadata(&path).map_err(|e| format!("Failed to read replay set: {}", e))?;
        if metadata.len() > MAX_SET_BYTES {
            return Err("Replay set file is too large".to_string());
        }
        let content =
            fs::read_to_string(&path).map_err(|e| format!("Failed to read replay set: {}", e))?;
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse replay set: {}", e))
    }

    /// Refuses to overwrite an existing set. Recording twice under the same
    /// name would otherwise discard the first one — and its baseline with it —
    /// without a word.
    pub fn create(&self, slug: &str, set: &ReplaySet) -> Result<PathBuf, String> {
        if self.file_path(slug)?.exists() {
            return Err(format!("A replay set named '{slug}' already exists"));
        }
        self.save(slug, set)
    }

    pub fn save(&self, slug: &str, set: &ReplaySet) -> Result<PathBuf, String> {
        let path = self.file_path(slug)?;
        fs::create_dir_all(&self.dir)
            .map_err(|e| format!("Failed to create replays directory: {}", e))?;
        let content = serde_json::to_string_pretty(set)
            .map_err(|e| format!("Failed to serialize replay set: {}", e))?;
        fs::write(&path, content).map_err(|e| format!("Failed to write replay set: {}", e))?;
        Ok(path)
    }

    pub fn delete(&self, slug: &str) -> Result<PathBuf, String> {
        let path = self.file_path(slug)?;
        fs::remove_file(&path).map_err(|e| format!("Failed to delete replay set: {}", e))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::types::{ExpectedOutcome, REPLAY_SET_VERSION, ReplayEntry, ReplaySource};

    fn sample_set() -> ReplaySet {
        ReplaySet {
            version: REPLAY_SET_VERSION,
            name: "checkout flow".to_string(),
            created_at: "2026-08-21T10:00:00Z".to_string(),
            source: ReplaySource {
                driver_id: "postgres".to_string(),
                connection_label: Some("staging".to_string()),
                environment: "staging".to_string(),
            },
            ignored_columns: vec!["updated_at".to_string()],
            redacted: false,
            entries: vec![ReplayEntry {
                id: uuid::Uuid::new_v4().to_string(),
                order: 1,
                query: "SELECT id FROM orders".to_string(),
                driver_id: "postgres".to_string(),
                namespace: None,
                operation_type: "select".to_string(),
                is_mutation: false,
                expected: ExpectedOutcome {
                    execution_time_ms: 12.4,
                    row_count: Some(42),
                    success: true,
                    fingerprint: Some("abcdef".to_string()),
                    result_digest: Some("sha256:deadbeef".to_string()),
                },
            }],
        }
    }

    #[test]
    fn rejects_path_traversal_slugs() {
        let store = ReplaySetStore::new(Path::new("/tmp/qoredb_test_replays"));
        assert!(store.load("../../../etc/passwd").is_err());
        assert!(store.delete("../../../etc/passwd").is_err());
        assert!(store.save("foo/bar", &sample_set()).is_err());
        assert!(store.load("").is_err());
        assert!(store.load("a b").is_err());
    }

    #[test]
    fn slugify_produces_a_valid_stem() {
        assert_eq!(slugify("Checkout Flow").unwrap(), "checkout-flow");
        assert_eq!(slugify("  order__totals  ").unwrap(), "order-totals");
        assert!(slugify("   ").is_err());
        assert!(slugify("///").is_err());
    }

    /// The counterpart of the test above: query literals *are* versioned, on
    /// purpose. This pins the behaviour so nobody reads the previous test as a
    /// guarantee that a set carries no sensitive value at all.
    #[test]
    fn persisted_set_keeps_query_literals_verbatim() {
        let dir = std::env::temp_dir().join(format!("qoredb_replay_{}", uuid::Uuid::new_v4()));
        let store = ReplaySetStore::new(&dir);

        let mut set = sample_set();
        set.entries[0].query = "SELECT * FROM users WHERE api_key = 'hunter2'".to_string();
        let path = store.save("literals", &set).unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            raw.contains("hunter2"),
            "a replayable query cannot be redacted; the set carries it verbatim"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn creating_over_an_existing_set_is_refused() {
        let dir = std::env::temp_dir().join(format!("qoredb_replay_{}", uuid::Uuid::new_v4()));
        let store = ReplaySetStore::new(&dir);

        store.create("checkout-flow", &sample_set()).unwrap();
        let err = store.create("checkout-flow", &sample_set()).unwrap_err();
        assert!(err.contains("already exists"));

        // Updating a set the caller already loaded stays possible.
        store.save("checkout-flow", &sample_set()).unwrap();

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn round_trips_a_set_through_disk() {
        let dir = std::env::temp_dir().join(format!("qoredb_replay_{}", uuid::Uuid::new_v4()));
        let store = ReplaySetStore::new(&dir);
        store.save("checkout-flow", &sample_set()).unwrap();

        let loaded = store.load("checkout-flow").unwrap();
        assert_eq!(loaded.name, "checkout flow");
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.ignored_columns, vec!["updated_at".to_string()]);

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].slug, "checkout-flow");
        assert_eq!(listed[0].entry_count, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The set is committed to the repo, so it must never carry *result rows* —
    /// only queries and expectations. Literals inside the query text are a
    /// separate, documented trade-off, covered by
    /// `persisted_set_keeps_query_literals_verbatim`.
    #[test]
    fn persisted_set_contains_no_row_values() {
        let dir = std::env::temp_dir().join(format!("qoredb_replay_{}", uuid::Uuid::new_v4()));
        let store = ReplaySetStore::new(&dir);
        let path = store.save("checkout-flow", &sample_set()).unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("\"rows\""));
        assert!(!raw.contains("\"values\""));
        assert!(!raw.contains("\"columns\""));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
