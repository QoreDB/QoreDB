//! Full-text Search Strategies
//!
//! Provides driver-specific optimized full-text search implementations
//! with automatic fallback to basic LIKE/regex search.
//!
//! Features:
//! - Auto-detection of full-text indexes
//! - Caching of table capabilities
//! - Driver-specific optimizations

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::engine::types::{Namespace, Value};

/// Cache TTL for table capabilities
const CAPABILITY_CACHE_TTL: Duration = Duration::from_secs(300);

/// Information about a column's full-text search capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSearchInfo {
    pub name: String,
    pub data_type: String,
    pub has_fulltext_index: bool,
    pub fulltext_index_name: Option<String>,
}

/// Search method used for a query
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SearchMethod {
    /// Native full-text search (tsvector, FULLTEXT, $text)
    NativeFulltext,
    /// Basic pattern matching (LIKE, ILIKE, $regex)
    PatternMatch,
    /// Hybrid: some columns use native, others use pattern
    Hybrid,
}

/// Options for executing a search on a table
#[derive(Debug, Clone)]
pub struct TableSearchOptions {
    pub search_term: String,
    pub case_sensitive: bool,
    pub max_results: u32,
    pub timeout_ms: Option<u64>,
    pub prefer_native: bool,
}

impl Default for TableSearchOptions {
    fn default() -> Self {
        Self {
            search_term: String::new(),
            case_sensitive: false,
            max_results: 10,
            timeout_ms: Some(5000),
            prefer_native: true,
        }
    }
}

/// Result of analyzing a table's search capabilities
#[derive(Debug, Clone)]
pub struct TableSearchCapability {
    pub searchable_columns: Vec<ColumnSearchInfo>,
    pub recommended_method: SearchMethod,
    pub estimated_rows: Option<u64>,
    pub has_any_fulltext_index: bool,
}

/// Cached capability
#[derive(Debug, Clone)]
struct CachedCapability {
    capability: TableSearchCapability,
    cached_at: Instant,
}

/// Global capability cache
#[derive(Debug)]
pub struct CapabilityCache {
    cache: RwLock<HashMap<String, CachedCapability>>,
}

impl CapabilityCache {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }

    fn make_key(namespace: &Namespace, table_name: &str) -> String {
        format!(
            "{}:{}:{}",
            namespace.database,
            namespace.schema.as_deref().unwrap_or(""),
            table_name
        )
    }

    pub async fn get(
        &self,
        namespace: &Namespace,
        table_name: &str,
    ) -> Option<TableSearchCapability> {
        let key = Self::make_key(namespace, table_name);
        let cache = self.cache.read().await;

        if let Some(cached) = cache.get(&key) {
            if cached.cached_at.elapsed() < CAPABILITY_CACHE_TTL {
                return Some(cached.capability.clone());
            }
        }
        None
    }

    pub async fn set(
        &self,
        namespace: &Namespace,
        table_name: &str,
        capability: TableSearchCapability,
    ) {
        let key = Self::make_key(namespace, table_name);
        let mut cache = self.cache.write().await;

        cache.insert(
            key,
            CachedCapability {
                capability,
                cached_at: Instant::now(),
            },
        );

        // Clean up old entries
        // TODO: Use a better eviction strategy if needed
        if cache.len() > 1000 {
            let now = Instant::now();
            cache.retain(|_, v| now.duration_since(v.cached_at) < CAPABILITY_CACHE_TTL);
        }
    }

    pub async fn invalidate(&self, namespace: &Namespace, table_name: &str) {
        let key = Self::make_key(namespace, table_name);
        let mut cache = self.cache.write().await;
        cache.remove(&key);
    }

    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

impl Default for CapabilityCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Lazy-initialized global cache
static CAPABILITY_CACHE: std::sync::OnceLock<Arc<CapabilityCache>> = std::sync::OnceLock::new();

pub fn get_capability_cache() -> Arc<CapabilityCache> {
    CAPABILITY_CACHE
        .get_or_init(|| Arc::new(CapabilityCache::new()))
        .clone()
}

/// Information needed to detect full-text indexes
#[derive(Debug, Clone)]
pub struct FulltextIndexInfo {
    pub index_name: String,
    pub columns: Vec<String>,
    pub index_type: String,
}

/// Trait for driver-specific full-text search strategies
#[async_trait]
pub trait FulltextSearchStrategy: Send + Sync {
    /// Get the driver identifier
    fn driver_id(&self) -> &str;

    /// Build a query to detect full-text indexes on a table
    /// Returns None if the driver doesn't support index detection
    fn build_index_detection_query(
        &self,
        namespace: &Namespace,
        table_name: &str,
    ) -> Option<String>;

    /// Parse the result of index detection query into FulltextIndexInfo
    fn parse_index_detection_result(
        &self,
        rows: &[Vec<Value>],
        columns: &[String],
    ) -> Vec<FulltextIndexInfo>;

    /// Build search capability from detected indexes
    fn build_capability(
        &self,
        text_columns: &[String],
        detected_indexes: &[FulltextIndexInfo],
        estimated_rows: Option<u64>,
    ) -> TableSearchCapability;

    /// Build an optimized search query for a table
    /// Returns (query_string, search_method_used)
    fn build_search_query(
        &self,
        namespace: &Namespace,
        table_name: &str,
        capability: &TableSearchCapability,
        options: &TableSearchOptions,
    ) -> (String, SearchMethod);

    /// Build a fallback LIKE-based query (always available)
    fn build_fallback_query(
        &self,
        namespace: &Namespace,
        table_name: &str,
        columns: &[String],
        options: &TableSearchOptions,
    ) -> String;
}

// ============================================
// POSTGRESQL STRATEGY
// ============================================

pub struct PostgresSearchStrategy;

impl PostgresSearchStrategy {
    pub fn new() -> Self {
        Self
    }

    fn quote_identifier(name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }

    fn escape_like_pattern(term: &str) -> String {
        term.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
            .replace('\'', "''")
    }

    fn escape_tsquery(term: &str) -> String {
        // Escape special tsquery characters
        let escaped = term
            .replace('\\', "")
            .replace('\'', "")
            .replace('&', "")
            .replace('|', "")
            .replace('!', "")
            .replace('(', "")
            .replace(')', "")
            .replace(':', "")
            .replace('*', "")
            .replace('<', "")
            .replace('>', "");

        // Split into words and join with & for AND search
        let words: Vec<&str> = escaped.split_whitespace().filter(|w| !w.is_empty()).collect();
        if words.is_empty() {
            "''".to_string()
        } else if words.len() == 1 {
            format!("'{}:*'", words[0])
        } else {
            let parts: Vec<String> = words.iter().map(|w| format!("{}:*", w)).collect();
            format!("'{}'", parts.join(" & "))
        }
    }
}

impl Default for PostgresSearchStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FulltextSearchStrategy for PostgresSearchStrategy {
    fn driver_id(&self) -> &str {
        "postgres"
    }

    fn build_index_detection_query(
        &self,
        namespace: &Namespace,
        table_name: &str,
    ) -> Option<String> {
        let schema = namespace.schema.as_deref().unwrap_or("public");
        // Query to find GIN/GiST indexes that might be used for full-text search
        // Also checks for tsvector columns
        Some(format!(
            r#"
            SELECT
                i.relname as index_name,
                a.attname as column_name,
                am.amname as index_type,
                COALESCE(
                    (SELECT pg_get_indexdef(i.oid)),
                    ''
                ) as index_def
            FROM pg_class t
            JOIN pg_namespace n ON n.oid = t.relnamespace
            JOIN pg_index ix ON t.oid = ix.indrelid
            JOIN pg_class i ON i.oid = ix.indexrelid
            JOIN pg_am am ON am.oid = i.relam
            JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
            WHERE n.nspname = '{}'
              AND t.relname = '{}'
              AND am.amname IN ('gin', 'gist')
              AND (
                  pg_get_indexdef(i.oid) LIKE '%tsvector%'
                  OR pg_get_indexdef(i.oid) LIKE '%to_tsvector%'
                  OR EXISTS (
                      SELECT 1 FROM pg_attribute ta
                      WHERE ta.attrelid = t.oid
                        AND ta.attname = a.attname
                        AND ta.atttypid = 'tsvector'::regtype
                  )
              )
            ORDER BY i.relname, a.attnum
            "#,
            schema.replace('\'', "''"),
            table_name.replace('\'', "''")
        ))
    }

    fn parse_index_detection_result(
        &self,
        rows: &[Vec<Value>],
        _columns: &[String],
    ) -> Vec<FulltextIndexInfo> {
        let mut indexes: HashMap<String, FulltextIndexInfo> = HashMap::new();

        for row in rows {
            if row.len() >= 3 {
                let index_name = match &row[0] {
                    Value::Text(s) => s.clone(),
                    _ => continue,
                };
                let column_name = match &row[1] {
                    Value::Text(s) => s.clone(),
                    _ => continue,
                };
                let index_type = match &row[2] {
                    Value::Text(s) => s.clone(),
                    _ => "gin".to_string(),
                };

                indexes
                    .entry(index_name.clone())
                    .or_insert_with(|| FulltextIndexInfo {
                        index_name,
                        columns: Vec::new(),
                        index_type,
                    })
                    .columns
                    .push(column_name);
            }
        }

        indexes.into_values().collect()
    }

    fn build_capability(
        &self,
        text_columns: &[String],
        detected_indexes: &[FulltextIndexInfo],
        estimated_rows: Option<u64>,
    ) -> TableSearchCapability {
        // Build a set of columns that have full-text indexes
        let indexed_columns: HashMap<String, String> = detected_indexes
            .iter()
            .flat_map(|idx| {
                idx.columns
                    .iter()
                    .map(|col| (col.clone(), idx.index_name.clone()))
            })
            .collect();

        let searchable_columns: Vec<ColumnSearchInfo> = text_columns
            .iter()
            .map(|name| {
                let index_info = indexed_columns.get(name);
                ColumnSearchInfo {
                    name: name.clone(),
                    data_type: "text".to_string(),
                    has_fulltext_index: index_info.is_some(),
                    fulltext_index_name: index_info.cloned(),
                }
            })
            .collect();

        let has_any_fulltext = searchable_columns.iter().any(|c| c.has_fulltext_index);

        let recommended_method = if has_any_fulltext {
            if searchable_columns.iter().all(|c| c.has_fulltext_index) {
                SearchMethod::NativeFulltext
            } else {
                SearchMethod::Hybrid
            }
        } else {
            SearchMethod::PatternMatch
        };

        TableSearchCapability {
            searchable_columns,
            recommended_method,
            estimated_rows,
            has_any_fulltext_index: has_any_fulltext,
        }
    }

    fn build_search_query(
        &self,
        namespace: &Namespace,
        table_name: &str,
        capability: &TableSearchCapability,
        options: &TableSearchOptions,
    ) -> (String, SearchMethod) {
        if capability.has_any_fulltext_index && options.prefer_native {
            let fulltext_cols: Vec<_> = capability
                .searchable_columns
                .iter()
                .filter(|c| c.has_fulltext_index)
                .collect();

            let pattern_cols: Vec<_> = capability
                .searchable_columns
                .iter()
                .filter(|c| !c.has_fulltext_index)
                .map(|c| c.name.clone())
                .collect();

            let mut conditions = Vec::new();

            // Full-text conditions using to_tsvector for flexibility
            for col in &fulltext_cols {
                let quoted = Self::quote_identifier(&col.name);
                let tsquery = Self::escape_tsquery(&options.search_term);
                conditions.push(format!(
                    "to_tsvector('simple', COALESCE({}::text, '')) @@ to_tsquery('simple', {})",
                    quoted, tsquery
                ));
            }

            // Pattern match for non-indexed columns
            if !pattern_cols.is_empty() {
                let pattern = format!("%{}%", Self::escape_like_pattern(&options.search_term));
                for col in &pattern_cols {
                    let quoted = Self::quote_identifier(col);
                    if options.case_sensitive {
                        conditions.push(format!("{}::text LIKE '{}'", quoted, pattern));
                    } else {
                        conditions.push(format!("{}::text ILIKE '{}'", quoted, pattern));
                    }
                }
            }

            let full_table = if let Some(schema) = &namespace.schema {
                format!(
                    "{}.{}",
                    Self::quote_identifier(schema),
                    Self::quote_identifier(table_name)
                )
            } else {
                Self::quote_identifier(table_name)
            };

            let query = format!(
                "SELECT * FROM {} WHERE {} LIMIT {}",
                full_table,
                conditions.join(" OR "),
                options.max_results
            );

            let method = if pattern_cols.is_empty() {
                SearchMethod::NativeFulltext
            } else {
                SearchMethod::Hybrid
            };

            (query, method)
        } else {
            let columns: Vec<String> = capability
                .searchable_columns
                .iter()
                .map(|c| c.name.clone())
                .collect();
            (
                self.build_fallback_query(namespace, table_name, &columns, options),
                SearchMethod::PatternMatch,
            )
        }
    }

    fn build_fallback_query(
        &self,
        namespace: &Namespace,
        table_name: &str,
        columns: &[String],
        options: &TableSearchOptions,
    ) -> String {
        let pattern = format!("%{}%", Self::escape_like_pattern(&options.search_term));

        let conditions: Vec<String> = columns
            .iter()
            .map(|col| {
                let quoted = Self::quote_identifier(col);
                if options.case_sensitive {
                    format!("{}::text LIKE '{}'", quoted, pattern)
                } else {
                    format!("{}::text ILIKE '{}'", quoted, pattern)
                }
            })
            .collect();

        let full_table = if let Some(schema) = &namespace.schema {
            format!(
                "{}.{}",
                Self::quote_identifier(schema),
                Self::quote_identifier(table_name)
            )
        } else {
            Self::quote_identifier(table_name)
        };

        format!(
            "SELECT * FROM {} WHERE {} LIMIT {}",
            full_table,
            conditions.join(" OR "),
            options.max_results
        )
    }
}

// ============================================
// MYSQL STRATEGY
// ============================================

pub struct MySqlSearchStrategy;

impl MySqlSearchStrategy {
    pub fn new() -> Self {
        Self
    }

    fn quote_identifier(name: &str) -> String {
        format!("`{}`", name.replace('`', "``"))
    }

    fn escape_like_pattern(term: &str) -> String {
        term.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
            .replace('\'', "''")
    }

    fn escape_fulltext(term: &str) -> String {
        term.replace('\\', "")
            .replace('\'', "")
            .replace('"', "")
            .replace('+', "")
            .replace('-', "")
            .replace('*', "")
            .replace('(', "")
            .replace(')', "")
            .replace('~', "")
            .replace('<', "")
            .replace('>', "")
            .replace('@', "")
    }
}

impl Default for MySqlSearchStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FulltextSearchStrategy for MySqlSearchStrategy {
    fn driver_id(&self) -> &str {
        "mysql"
    }

    fn build_index_detection_query(
        &self,
        namespace: &Namespace,
        table_name: &str,
    ) -> Option<String> {
        // Query to find FULLTEXT indexes
        Some(format!(
            r#"
            SELECT
                INDEX_NAME as index_name,
                COLUMN_NAME as column_name,
                INDEX_TYPE as index_type
            FROM INFORMATION_SCHEMA.STATISTICS
            WHERE TABLE_SCHEMA = '{}'
              AND TABLE_NAME = '{}'
              AND INDEX_TYPE = 'FULLTEXT'
            ORDER BY INDEX_NAME, SEQ_IN_INDEX
            "#,
            namespace.database.replace('\'', "''"),
            table_name.replace('\'', "''")
        ))
    }

    fn parse_index_detection_result(
        &self,
        rows: &[Vec<Value>],
        _columns: &[String],
    ) -> Vec<FulltextIndexInfo> {
        let mut indexes: HashMap<String, FulltextIndexInfo> = HashMap::new();

        for row in rows {
            if row.len() >= 3 {
                let index_name = match &row[0] {
                    Value::Text(s) => s.clone(),
                    _ => continue,
                };
                let column_name = match &row[1] {
                    Value::Text(s) => s.clone(),
                    _ => continue,
                };
                let index_type = match &row[2] {
                    Value::Text(s) => s.clone(),
                    _ => "FULLTEXT".to_string(),
                };

                indexes
                    .entry(index_name.clone())
                    .or_insert_with(|| FulltextIndexInfo {
                        index_name,
                        columns: Vec::new(),
                        index_type,
                    })
                    .columns
                    .push(column_name);
            }
        }

        indexes.into_values().collect()
    }

    fn build_capability(
        &self,
        text_columns: &[String],
        detected_indexes: &[FulltextIndexInfo],
        estimated_rows: Option<u64>,
    ) -> TableSearchCapability {
        let indexed_columns: HashMap<String, String> = detected_indexes
            .iter()
            .flat_map(|idx| {
                idx.columns
                    .iter()
                    .map(|col| (col.clone(), idx.index_name.clone()))
            })
            .collect();

        let searchable_columns: Vec<ColumnSearchInfo> = text_columns
            .iter()
            .map(|name| {
                let index_info = indexed_columns.get(name);
                ColumnSearchInfo {
                    name: name.clone(),
                    data_type: "text".to_string(),
                    has_fulltext_index: index_info.is_some(),
                    fulltext_index_name: index_info.cloned(),
                }
            })
            .collect();

        let has_any_fulltext = searchable_columns.iter().any(|c| c.has_fulltext_index);

        let recommended_method = if has_any_fulltext {
            SearchMethod::NativeFulltext
        } else {
            SearchMethod::PatternMatch
        };

        TableSearchCapability {
            searchable_columns,
            recommended_method,
            estimated_rows,
            has_any_fulltext_index: has_any_fulltext,
        }
    }

    fn build_search_query(
        &self,
        namespace: &Namespace,
        table_name: &str,
        capability: &TableSearchCapability,
        options: &TableSearchOptions,
    ) -> (String, SearchMethod) {
        // Group columns by their FULLTEXT index
        let fulltext_cols: Vec<_> = capability
            .searchable_columns
            .iter()
            .filter(|c| c.has_fulltext_index)
            .collect();

        if !fulltext_cols.is_empty() && options.prefer_native {
            // Group by index name for MATCH...AGAINST
            let mut index_groups: HashMap<String, Vec<String>> = HashMap::new();
            for col in &fulltext_cols {
                if let Some(idx_name) = &col.fulltext_index_name {
                    index_groups
                        .entry(idx_name.clone())
                        .or_default()
                        .push(col.name.clone());
                }
            }

            let mut conditions = Vec::new();
            let search_term = Self::escape_fulltext(&options.search_term);

            for (_idx_name, cols) in &index_groups {
                let cols_str = cols
                    .iter()
                    .map(|c| Self::quote_identifier(c))
                    .collect::<Vec<_>>()
                    .join(", ");

                // Use BOOLEAN MODE with wildcards for partial matching
                conditions.push(format!(
                    "MATCH({}) AGAINST('*{}*' IN BOOLEAN MODE)",
                    cols_str, search_term
                ));
            }

            // Add LIKE for non-indexed columns
            let pattern_cols: Vec<_> = capability
                .searchable_columns
                .iter()
                .filter(|c| !c.has_fulltext_index)
                .collect();

            if !pattern_cols.is_empty() {
                let pattern = format!("%{}%", Self::escape_like_pattern(&options.search_term));
                for col in &pattern_cols {
                    let quoted = Self::quote_identifier(&col.name);
                    conditions.push(format!("{} LIKE '{}'", quoted, pattern));
                }
            }

            let full_table = format!(
                "{}.{}",
                Self::quote_identifier(&namespace.database),
                Self::quote_identifier(table_name)
            );

            let query = format!(
                "SELECT * FROM {} WHERE {} LIMIT {}",
                full_table,
                conditions.join(" OR "),
                options.max_results
            );

            let method = if pattern_cols.is_empty() {
                SearchMethod::NativeFulltext
            } else {
                SearchMethod::Hybrid
            };

            (query, method)
        } else {
            let columns: Vec<String> = capability
                .searchable_columns
                .iter()
                .map(|c| c.name.clone())
                .collect();
            (
                self.build_fallback_query(namespace, table_name, &columns, options),
                SearchMethod::PatternMatch,
            )
        }
    }

    fn build_fallback_query(
        &self,
        namespace: &Namespace,
        table_name: &str,
        columns: &[String],
        options: &TableSearchOptions,
    ) -> String {
        let pattern = format!("%{}%", Self::escape_like_pattern(&options.search_term));

        let conditions: Vec<String> = columns
            .iter()
            .map(|col| {
                let quoted = Self::quote_identifier(col);
                if options.case_sensitive {
                    format!("{} LIKE '{}' COLLATE utf8mb4_bin", quoted, pattern)
                } else {
                    format!("{} LIKE '{}'", quoted, pattern)
                }
            })
            .collect();

        let full_table = format!(
            "{}.{}",
            Self::quote_identifier(&namespace.database),
            Self::quote_identifier(table_name)
        );

        format!(
            "SELECT * FROM {} WHERE {} LIMIT {}",
            full_table,
            conditions.join(" OR "),
            options.max_results
        )
    }
}

// ============================================
// SQLITE STRATEGY
// ============================================

pub struct SqliteSearchStrategy;

impl SqliteSearchStrategy {
    pub fn new() -> Self {
        Self
    }

    fn quote_identifier(name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }

    fn escape_like_pattern(term: &str) -> String {
        term.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
            .replace('\'', "''")
    }

    fn escape_sql_literal(term: &str) -> String {
        term.replace('\'', "''")
    }

    fn escape_match(term: &str) -> String {
        let cleaned = term
            .replace('\\', "")
            .replace('\'', "")
            .replace('"', "")
            .replace(':', " ")
            .replace('*', " ")
            .replace('-', " ")
            .replace('+', " ")
            .replace('~', " ")
            .replace('(', " ")
            .replace(')', " ")
            .replace('<', " ")
            .replace('>', " ");

        cleaned
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl Default for SqliteSearchStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FulltextSearchStrategy for SqliteSearchStrategy {
    fn driver_id(&self) -> &str {
        "sqlite"
    }

    fn build_index_detection_query(
        &self,
        _namespace: &Namespace,
        table_name: &str,
    ) -> Option<String> {
        let table_name = table_name.replace('\'', "''");
        let like_content_single = format!("%content=''{}''%", table_name);
        let like_content_double = format!("%content=\"{}\"%", table_name);
        let like_content_plain = format!("%content={}%", table_name);

        Some(format!(
            r#"
            SELECT
                m.name as index_name,
                p.name as column_name,
                'fts' as index_type
            FROM sqlite_master m
            JOIN pragma_table_info(m.name) p
            WHERE m.type = 'table'
              AND m.sql IS NOT NULL
              AND m.sql LIKE 'CREATE VIRTUAL TABLE %USING fts%'
              AND (
                m.name = '{table_name}'
                OR m.sql LIKE '{like_content_single}'
                OR m.sql LIKE '{like_content_double}'
                OR m.sql LIKE '{like_content_plain}'
              )
              AND p.name NOT IN ('rowid')
            ORDER BY m.name, p.cid
            "#,
            table_name = table_name,
            like_content_single = like_content_single,
            like_content_double = like_content_double,
            like_content_plain = like_content_plain,
        ))
    }

    fn parse_index_detection_result(
        &self,
        rows: &[Vec<Value>],
        _columns: &[String],
    ) -> Vec<FulltextIndexInfo> {
        let mut indexes: HashMap<String, FulltextIndexInfo> = HashMap::new();

        for row in rows {
            if row.len() >= 3 {
                let index_name = match &row[0] {
                    Value::Text(s) => s.clone(),
                    _ => continue,
                };
                let column_name = match &row[1] {
                    Value::Text(s) => s.clone(),
                    _ => continue,
                };
                let index_type = match &row[2] {
                    Value::Text(s) => s.clone(),
                    _ => "fts".to_string(),
                };

                indexes
                    .entry(index_name.clone())
                    .or_insert_with(|| FulltextIndexInfo {
                        index_name,
                        columns: Vec::new(),
                        index_type,
                    })
                    .columns
                    .push(column_name);
            }
        }

        indexes.into_values().collect()
    }

    fn build_capability(
        &self,
        text_columns: &[String],
        detected_indexes: &[FulltextIndexInfo],
        estimated_rows: Option<u64>,
    ) -> TableSearchCapability {
        let indexed_columns: HashMap<String, String> = detected_indexes
            .iter()
            .flat_map(|idx| {
                idx.columns
                    .iter()
                    .map(|col| (col.clone(), idx.index_name.clone()))
            })
            .collect();

        let searchable_columns: Vec<ColumnSearchInfo> = text_columns
            .iter()
            .map(|name| {
                let index_info = indexed_columns.get(name);
                ColumnSearchInfo {
                    name: name.clone(),
                    data_type: "text".to_string(),
                    has_fulltext_index: index_info.is_some(),
                    fulltext_index_name: index_info.cloned(),
                }
            })
            .collect();

        let has_any_fulltext = searchable_columns.iter().any(|c| c.has_fulltext_index);

        let recommended_method = if has_any_fulltext {
            SearchMethod::NativeFulltext
        } else {
            SearchMethod::PatternMatch
        };

        TableSearchCapability {
            searchable_columns,
            recommended_method,
            estimated_rows,
            has_any_fulltext_index: has_any_fulltext,
        }
    }

    fn build_search_query(
        &self,
        _namespace: &Namespace,
        table_name: &str,
        capability: &TableSearchCapability,
        options: &TableSearchOptions,
    ) -> (String, SearchMethod) {
        let fulltext_cols: Vec<_> = capability
            .searchable_columns
            .iter()
            .filter(|c| c.has_fulltext_index)
            .collect();

        let pattern_cols: Vec<_> = capability
            .searchable_columns
            .iter()
            .filter(|c| !c.has_fulltext_index)
            .collect();

        if !fulltext_cols.is_empty() && options.prefer_native {
            let fts_table = fulltext_cols
                .iter()
                .find_map(|c| c.fulltext_index_name.as_ref())
                .cloned();

            if let Some(fts_table) = fts_table {
                let match_term = Self::escape_match(&options.search_term);
                if match_term.is_empty() {
                    let columns: Vec<String> = capability
                        .searchable_columns
                        .iter()
                        .map(|c| c.name.clone())
                        .collect();
                    return (
                        self.build_fallback_query(_namespace, table_name, &columns, options),
                        SearchMethod::PatternMatch,
                    );
                }

                let base_table = Self::quote_identifier(table_name);
                let base_alias = "t";
                let fts_alias = "fts";
                let mut conditions = Vec::new();

                let match_target = if fts_table == table_name {
                    base_alias.to_string()
                } else {
                    fts_alias.to_string()
                };
                conditions.push(format!("{} MATCH '{}'", match_target, match_term));

                if !pattern_cols.is_empty() {
                    let term = Self::escape_sql_literal(&options.search_term);
                    for col in &pattern_cols {
                        let col_ref = format!("{}.{}", base_alias, Self::quote_identifier(&col.name));
                        let clause = if options.case_sensitive {
                            format!(
                                "instr(CAST({} AS TEXT), '{}') > 0",
                                col_ref, term
                            )
                        } else {
                            format!(
                                "instr(LOWER(CAST({} AS TEXT)), LOWER('{}')) > 0",
                                col_ref, term
                            )
                        };
                        conditions.push(clause);
                    }
                }

                let where_sql = conditions.join(" OR ");

                let query = if fts_table == table_name {
                    format!(
                        "SELECT {}.* FROM {} {} WHERE {} LIMIT {}",
                        base_alias, base_table, base_alias, where_sql, options.max_results
                    )
                } else {
                    let fts_table = Self::quote_identifier(&fts_table);
                    format!(
                        "SELECT {}.* FROM {} {} JOIN {} {} ON {}.rowid = {}.rowid WHERE {} LIMIT {}",
                        base_alias,
                        base_table,
                        base_alias,
                        fts_table,
                        fts_alias,
                        base_alias,
                        fts_alias,
                        where_sql,
                        options.max_results
                    )
                };

                let method = if pattern_cols.is_empty() {
                    SearchMethod::NativeFulltext
                } else {
                    SearchMethod::Hybrid
                };

                return (query, method);
            }
        }

        let columns: Vec<String> = capability
            .searchable_columns
            .iter()
            .map(|c| c.name.clone())
            .collect();
        (
            self.build_fallback_query(_namespace, table_name, &columns, options),
            SearchMethod::PatternMatch,
        )
    }

    fn build_fallback_query(
        &self,
        _namespace: &Namespace,
        table_name: &str,
        columns: &[String],
        options: &TableSearchOptions,
    ) -> String {
        let term = Self::escape_sql_literal(&options.search_term);

        let conditions: Vec<String> = columns
            .iter()
            .map(|col| {
                let quoted = Self::quote_identifier(col);
                if options.case_sensitive {
                    format!("instr(CAST({} AS TEXT), '{}') > 0", quoted, term)
                } else {
                    format!(
                        "instr(LOWER(CAST({} AS TEXT)), LOWER('{}')) > 0",
                        quoted, term
                    )
                }
            })
            .collect();

        let table = Self::quote_identifier(table_name);

        format!(
            "SELECT * FROM {} WHERE {} LIMIT {}",
            table,
            conditions.join(" OR "),
            options.max_results
        )
    }
}

// ============================================
// MONGODB STRATEGY
// ============================================

pub struct MongoSearchStrategy;

impl MongoSearchStrategy {
    pub fn new() -> Self {
        Self
    }

    fn escape_regex(term: &str) -> String {
        let special_chars = [
            '.', '^', '$', '*', '+', '?', '(', ')', '[', ']', '{', '}', '|', '\\',
        ];
        let mut escaped = String::with_capacity(term.len() * 2);
        for c in term.chars() {
            if special_chars.contains(&c) {
                escaped.push('\\');
            }
            escaped.push(c);
        }
        // Also escape quotes for JSON
        escaped.replace('"', "\\\"")
    }
}

impl Default for MongoSearchStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FulltextSearchStrategy for MongoSearchStrategy {
    fn driver_id(&self) -> &str {
        "mongodb"
    }

    fn build_index_detection_query(
        &self,
        _namespace: &Namespace,
        _table_name: &str,
    ) -> Option<String> {
        // MongoDB uses listIndexes command, not a query
        // We'll handle this separately in the search command
        None
    }

    fn parse_index_detection_result(
        &self,
        _rows: &[Vec<Value>],
        _columns: &[String],
    ) -> Vec<FulltextIndexInfo> {
        // Not used for MongoDB (handled differently)
        Vec::new()
    }

    fn build_capability(
        &self,
        text_columns: &[String],
        detected_indexes: &[FulltextIndexInfo],
        estimated_rows: Option<u64>,
    ) -> TableSearchCapability {
        // Check if there's a text index
        let has_text_index = detected_indexes
            .iter()
            .any(|idx| idx.index_type == "text");

        let searchable_columns: Vec<ColumnSearchInfo> = text_columns
            .iter()
            .map(|name| ColumnSearchInfo {
                name: name.clone(),
                data_type: "string".to_string(),
                has_fulltext_index: has_text_index,
                fulltext_index_name: if has_text_index {
                    detected_indexes.first().map(|i| i.index_name.clone())
                } else {
                    None
                },
            })
            .collect();

        let recommended_method = if has_text_index {
            SearchMethod::NativeFulltext
        } else {
            SearchMethod::PatternMatch
        };

        TableSearchCapability {
            searchable_columns,
            recommended_method,
            estimated_rows,
            has_any_fulltext_index: has_text_index,
        }
    }

    fn build_search_query(
        &self,
        _namespace: &Namespace,
        _table_name: &str,
        capability: &TableSearchCapability,
        options: &TableSearchOptions,
    ) -> (String, SearchMethod) {
        if capability.has_any_fulltext_index && options.prefer_native {
            // Use $text search (requires text index on collection)
            let search_term = options
                .search_term
                .replace('\\', "\\\\")
                .replace('"', "\\\"");

            let query = format!(
                r#"{{ "$text": {{ "$search": "{}" }} }}.limit({})"#,
                search_term, options.max_results
            );
            (query, SearchMethod::NativeFulltext)
        } else {
            let columns: Vec<String> = capability
                .searchable_columns
                .iter()
                .map(|c| c.name.clone())
                .collect();
            (
                self.build_fallback_query(_namespace, _table_name, &columns, options),
                SearchMethod::PatternMatch,
            )
        }
    }

    fn build_fallback_query(
        &self,
        _namespace: &Namespace,
        _table_name: &str,
        columns: &[String],
        options: &TableSearchOptions,
    ) -> String {
        let regex_options = if options.case_sensitive { "" } else { "i" };
        let escaped_term = Self::escape_regex(&options.search_term);

        let conditions: Vec<String> = columns
            .iter()
            .map(|col| {
                format!(
                    r#"{{ "{}": {{ "$regex": "{}", "$options": "{}" }} }}"#,
                    col, escaped_term, regex_options
                )
            })
            .collect();

        format!(
            r#"{{ "$or": [{}] }}.limit({})"#,
            conditions.join(", "),
            options.max_results
        )
    }
}

// ============================================
// STRATEGY FACTORY
// ============================================

/// Get the appropriate search strategy for a driver
pub fn get_search_strategy(driver_id: &str) -> Box<dyn FulltextSearchStrategy> {
    match driver_id.to_lowercase().as_str() {
        "postgres" | "postgresql" => Box::new(PostgresSearchStrategy::new()),
        "mysql" | "mariadb" => Box::new(MySqlSearchStrategy::new()),
        "sqlite" | "sqlite3" => Box::new(SqliteSearchStrategy::new()),
        "mongodb" | "mongo" => Box::new(MongoSearchStrategy::new()),
        _ => Box::new(PostgresSearchStrategy::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_escape_like() {
        assert_eq!(
            PostgresSearchStrategy::escape_like_pattern("test%user"),
            "test\\%user"
        );
        assert_eq!(
            PostgresSearchStrategy::escape_like_pattern("it's"),
            "it''s"
        );
    }

    #[test]
    fn test_postgres_escape_tsquery() {
        assert_eq!(PostgresSearchStrategy::escape_tsquery("hello"), "'hello:*'");
        assert_eq!(
            PostgresSearchStrategy::escape_tsquery("hello world"),
            "'hello:* & world:*'"
        );
    }

    #[test]
    fn test_mysql_escape_like() {
        assert_eq!(
            MySqlSearchStrategy::escape_like_pattern("test_user"),
            "test\\_user"
        );
    }

    #[test]
    fn test_sqlite_escape_like() {
        assert_eq!(
            SqliteSearchStrategy::escape_like_pattern("test%user"),
            "test\\%user"
        );
    }

    #[test]
    fn test_mongo_escape_regex() {
        assert_eq!(MongoSearchStrategy::escape_regex("test.user"), "test\\.user");
        assert_eq!(MongoSearchStrategy::escape_regex("(test)"), "\\(test\\)");
    }

    #[test]
    fn test_capability_cache_key() {
        let ns = Namespace {
            database: "mydb".to_string(),
            schema: Some("public".to_string()),
        };
        let key = CapabilityCache::make_key(&ns, "users");
        assert_eq!(key, "mydb:public:users");
    }

    #[tokio::test]
    async fn test_capability_cache() {
        let cache = CapabilityCache::new();
        let ns = Namespace {
            database: "test".to_string(),
            schema: None,
        };

        let capability = TableSearchCapability {
            searchable_columns: vec![],
            recommended_method: SearchMethod::PatternMatch,
            estimated_rows: Some(100),
            has_any_fulltext_index: false,
        };

        // Should be empty initially
        assert!(cache.get(&ns, "users").await.is_none());

        // Set and get
        cache.set(&ns, "users", capability.clone()).await;
        let cached = cache.get(&ns, "users").await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().estimated_rows, Some(100));

        // Invalidate
        cache.invalidate(&ns, "users").await;
        assert!(cache.get(&ns, "users").await.is_none());
    }
}
