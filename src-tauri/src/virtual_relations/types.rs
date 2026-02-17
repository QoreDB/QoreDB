// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

/// A user-defined virtual foreign key relation stored locally
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualRelation {
    /// Unique identifier for this relation
    pub id: String,
    /// The source table namespace (database name)
    pub source_database: String,
    /// The source table namespace (schema, if applicable)
    pub source_schema: Option<String>,
    /// The source table name
    pub source_table: String,
    /// The source column name
    pub source_column: String,
    /// The referenced table name
    pub referenced_table: String,
    /// The referenced column name
    pub referenced_column: String,
    /// The referenced schema (if different from source)
    pub referenced_schema: Option<String>,
    /// The referenced database (if different from source)
    pub referenced_database: Option<String>,
    /// Optional user-provided label for this relation
    pub label: Option<String>,
}

/// The on-disk format for virtual relations per connection
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
