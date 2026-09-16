// SPDX-License-Identifier: Apache-2.0

//! File-based workspaces (`.qoredb/`): discovery, connection store and the
//! write registry the desktop watcher uses to ignore its own writes.

use std::path::Path;

pub mod connection_store;
pub mod discovery;
pub mod query_library;
pub mod write_registry;

pub const DEFAULT_PROJECT_ID: &str = "default";

fn fnv1a_hash(data: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Project id of a file-based workspace, derived from its `.qoredb/` path.
/// FNV-1a keeps it stable across Rust versions; every surface must derive it
/// the same way because it names the keyring service holding the secrets.
pub fn workspace_project_id(qoredb_path: &Path) -> String {
    format!(
        "ws_{:016x}",
        fnv1a_hash(qoredb_path.to_string_lossy().as_bytes())
    )
}

/// Keyring service name for a project id.
pub fn keyring_service(project_id: &str) -> String {
    format!("qoredb_{project_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden values: credentials are keyed by these hashes, a change would
    /// lock every user out of their saved secrets.
    #[test]
    fn fnv1a_hash_stability() {
        assert_eq!(fnv1a_hash(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a_hash(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(
            fnv1a_hash(b"/Users/dev/project/.qoredb"),
            0x1c089eff0e6e433e
        );
        assert_eq!(fnv1a_hash(b"/home/user/app/.qoredb"), 0x49f7a110a4ef9f9b);
    }

    #[test]
    fn project_id_and_keyring_service_are_stable() {
        let id = workspace_project_id(Path::new("/home/user/app/.qoredb"));
        assert_eq!(id, "ws_49f7a110a4ef9f9b");
        assert_eq!(keyring_service(&id), "qoredb_ws_49f7a110a4ef9f9b");
    }
}
