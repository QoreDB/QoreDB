// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

/// User-defined virtual foreign key relation stored locally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualRelation {
    pub id: String,
    pub source_database: String,
    pub source_schema: Option<String>,
    pub source_table: String,
    pub source_column: String,
    pub referenced_table: String,
    pub referenced_column: String,
    /// Referenced schema, when different from the source.
    pub referenced_schema: Option<String>,
    /// Referenced database, when different from the source.
    pub referenced_database: Option<String>,
    /// Optional user-provided label.
    pub label: Option<String>,
}

/// On-disk format for virtual relations, one file per connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualRelationsConfig {
    pub version: u32,
    pub relations: Vec<VirtualRelation>,
}

impl Default for VirtualRelationsConfig {
    fn default() -> Self {
        Self {
            version: 1,
            relations: Vec::new(),
        }
    }
}
