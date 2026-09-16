// SPDX-License-Identifier: Apache-2.0

pub mod manager;
pub mod types;
pub mod watcher;

pub use qore_service::workspace::{connection_store, discovery, write_registry};

pub use manager::WorkspaceManager;
pub use types::{WorkspaceInfo, WorkspaceManifest, WorkspaceSource};
