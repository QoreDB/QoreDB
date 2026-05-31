// SPDX-License-Identifier: Apache-2.0

//! QoreService — surface-agnostic service layer for QorePlatform.
//!
//! Holds the Tauri-free business logic shared by every surface: the desktop
//! app (Tauri), the CLI/TUI, the MCP server, and `qore-server`. The app crate
//! provides only the thin IPC adapters (`commands/`) on top of this layer.
//!
//! Extraction is incremental (see `doc/private/JALON_0_QORE_SERVICE.md`): pure
//! cross-cutting modules land here first, then the orchestration logic lifted
//! out of `commands/`.

pub mod cache;
pub mod metrics;
pub mod paths;
pub mod policy;
pub mod ratelimit;
