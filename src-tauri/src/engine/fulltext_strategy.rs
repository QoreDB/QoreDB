//! Full-text Search Strategies
//!
//! Provides driver-specific optimized full-text search implementations
//! with automatic fallback to basic LIKE/regex search.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::engine::types::{Namespace, Value};

/// Information about a column's full-text search capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSearchInfo {
    pub name: String,
    pub data_type: String,
    /// Whether this column has a full-text index
    pub has_fulltext_index: bool,
    /// Name of the full-text index if any
    pub fulltext_index_name: Option<String>,
}

/// Result from a single table search
#[derive(Debug, Clone)]
pub struct TableSearchResult {
    pub table_name: String,
    pub column_name: String,
    pub value: Value,
    pub row_data: Vec<(String, Value)>,
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
    /// If true, prefer native full-text even if slower for small tables
    pub prefer_native: bool,
}

impl Default for TableSearchOptions {
    fn default() -> Self {
        Self {
            search_term: String::new(),
            case_sensitive: false,
            max_results: 10,
            timeout_ms: Some(5000), // 5 second timeout per table
            prefer_native: true,
        }
    }
}

/// Result of analyzing a table's search capabilities
#[derive(Debug, Clone)]
pub struct TableSearchCapability {
    /// Columns that can be searched
    pub searchable_columns: Vec<ColumnSearchInfo>,
    /// Recommended search method
    pub recommended_method: SearchMethod,
    /// Estimated row count (for optimization decisions)
    pub estimated_rows: Option<u64>,
}

/// Trait for driver-specific full-text search strategies
#[async_trait]
pub trait FulltextSearchStrategy: Send + Sync {
    /// Get the driver identifier
    fn driver_id(&self) -> &str;

    /// Analyze a table's full-text search capabilities
    async fn analyze_table(
        &self,
        namespace: &Namespace,
        table_name: &str,
        text_columns: &[String],
    ) -> Result<TableSearchCapability, String>;

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
        // Escape special tsquery characters and wrap in quotes for phrase search
        let escaped = term
            .replace('\\', "\\\\")
            .replace('\'', "''")
            .replace('&', "")
            .replace('|', "")
            .replace('!', "")
            .replace('(', "")
            .replace(')', "")
            .replace(':', "")
            .replace('*', "");

        // Split into words and join with & for AND search
        let words: Vec<&str> = escaped.split_whitespace().collect();
        if words.is_empty() {
            escaped
        } else if words.len() == 1 {
            format!("{}:*", words[0]) // Prefix search for single word
        } else {
            words.iter()
                .map(|w| format!("{}:*", w))
                .collect::<Vec<_>>()
                .join(" & ")
        }
    }
}

#[async_trait]
impl FulltextSearchStrategy for PostgresSearchStrategy {
    fn driver_id(&self) -> &str {
        "postgres"
    }

    async fn analyze_table(
        &self,
        _namespace: &Namespace,
        _table_name: &str,
        text_columns: &[String],
    ) -> Result<TableSearchCapability, String> {
        // For now, we assume no full-text indexes exist
        // In a full implementation, we'd query pg_indexes for GIN/GiST indexes on tsvector
        let searchable_columns = text_columns
            .iter()
            .map(|name| ColumnSearchInfo {
                name: name.clone(),
                data_type: "text".to_string(),
                has_fulltext_index: false,
                fulltext_index_name: None,
            })
            .collect();

        Ok(TableSearchCapability {
            searchable_columns,
            recommended_method: SearchMethod::PatternMatch,
            estimated_rows: None,
        })
    }

    fn build_search_query(
        &self,
        namespace: &Namespace,
        table_name: &str,
        capability: &TableSearchCapability,
        options: &TableSearchOptions,
    ) -> (String, SearchMethod) {
        let has_fulltext = capability
            .searchable_columns
            .iter()
            .any(|c| c.has_fulltext_index);

        if has_fulltext && options.prefer_native {
            // Use tsvector search for columns with full-text indexes
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

            // Full-text conditions
            for col in &fulltext_cols {
                let quoted = Self::quote_identifier(&col.name);
                let tsquery = Self::escape_tsquery(&options.search_term);
                conditions.push(format!(
                    "to_tsvector('simple', {}) @@ to_tsquery('simple', '{}')",
                    quoted, tsquery
                ));
            }

            // Pattern match for non-indexed columns
            if !pattern_cols.is_empty() {
                let pattern = format!("%{}%", Self::escape_like_pattern(&options.search_term));
                for col in &pattern_cols {
                    let quoted = Self::quote_identifier(col);
                    if options.case_sensitive {
                        conditions.push(format!("{} LIKE '{}'", quoted, pattern));
                    } else {
                        conditions.push(format!("{} ILIKE '{}'", quoted, pattern));
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
            // Fallback to ILIKE
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
                    format!("{} LIKE '{}'", quoted, pattern)
                } else {
                    format!("{} ILIKE '{}'", quoted, pattern)
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
        // Escape special FULLTEXT characters
        term.replace('\\', "\\\\")
            .replace('\'', "''")
            .replace('"', "\\\"")
            .replace('+', "")
            .replace('-', "")
            .replace('*', "")
            .replace('(', "")
            .replace(')', "")
            .replace('~', "")
            .replace('<', "")
            .replace('>', "")
    }
}

#[async_trait]
impl FulltextSearchStrategy for MySqlSearchStrategy {
    fn driver_id(&self) -> &str {
        "mysql"
    }

    async fn analyze_table(
        &self,
        _namespace: &Namespace,
        _table_name: &str,
        text_columns: &[String],
    ) -> Result<TableSearchCapability, String> {
        // For now, assume no FULLTEXT indexes
        // In a full implementation, we'd query SHOW INDEX for FULLTEXT type
        let searchable_columns = text_columns
            .iter()
            .map(|name| ColumnSearchInfo {
                name: name.clone(),
                data_type: "text".to_string(),
                has_fulltext_index: false,
                fulltext_index_name: None,
            })
            .collect();

        Ok(TableSearchCapability {
            searchable_columns,
            recommended_method: SearchMethod::PatternMatch,
            estimated_rows: None,
        })
    }

    fn build_search_query(
        &self,
        namespace: &Namespace,
        table_name: &str,
        capability: &TableSearchCapability,
        options: &TableSearchOptions,
    ) -> (String, SearchMethod) {
        let fulltext_cols: Vec<_> = capability
            .searchable_columns
            .iter()
            .filter(|c| c.has_fulltext_index)
            .collect();

        if !fulltext_cols.is_empty() && options.prefer_native {
            // Group columns by their fulltext index for MATCH...AGAINST
            let cols_str = fulltext_cols
                .iter()
                .map(|c| Self::quote_identifier(&c.name))
                .collect::<Vec<_>>()
                .join(", ");

            let search_term = Self::escape_fulltext(&options.search_term);

            let full_table = format!(
                "{}.{}",
                Self::quote_identifier(&namespace.database),
                Self::quote_identifier(table_name)
            );

            // Use BOOLEAN MODE for more flexible matching
            let query = format!(
                "SELECT * FROM {} WHERE MATCH({}) AGAINST('*{}*' IN BOOLEAN MODE) LIMIT {}",
                full_table, cols_str, search_term, options.max_results
            );

            (query, SearchMethod::NativeFulltext)
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
        escaped
    }
}

#[async_trait]
impl FulltextSearchStrategy for MongoSearchStrategy {
    fn driver_id(&self) -> &str {
        "mongodb"
    }

    async fn analyze_table(
        &self,
        _namespace: &Namespace,
        _table_name: &str,
        text_columns: &[String],
    ) -> Result<TableSearchCapability, String> {
        // For MongoDB, we'd check for text indexes via listIndexes
        // For now, assume no text indexes
        let searchable_columns = text_columns
            .iter()
            .map(|name| ColumnSearchInfo {
                name: name.clone(),
                data_type: "string".to_string(),
                has_fulltext_index: false,
                fulltext_index_name: None,
            })
            .collect();

        Ok(TableSearchCapability {
            searchable_columns,
            recommended_method: SearchMethod::PatternMatch,
            estimated_rows: None,
        })
    }

    fn build_search_query(
        &self,
        _namespace: &Namespace,
        _table_name: &str,
        capability: &TableSearchCapability,
        options: &TableSearchOptions,
    ) -> (String, SearchMethod) {
        let has_text_index = capability
            .searchable_columns
            .iter()
            .any(|c| c.has_fulltext_index);

        if has_text_index && options.prefer_native {
            // Use $text search (requires text index)
            let search_term = options.search_term.replace('"', "\\\"");
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
        "mongodb" | "mongo" => Box::new(MongoSearchStrategy::new()),
        // Fallback to PostgreSQL strategy (most compatible SQL)
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
    fn test_mysql_escape_like() {
        assert_eq!(
            MySqlSearchStrategy::escape_like_pattern("test_user"),
            "test\\_user"
        );
    }

    #[test]
    fn test_mongo_escape_regex() {
        assert_eq!(
            MongoSearchStrategy::escape_regex("test.user"),
            "test\\.user"
        );
        assert_eq!(
            MongoSearchStrategy::escape_regex("(test)"),
            "\\(test\\)"
        );
    }

    #[test]
    fn test_postgres_fallback_query() {
        let strategy = PostgresSearchStrategy::new();
        let ns = Namespace {
            database: "mydb".to_string(),
            schema: Some("public".to_string()),
        };
        let options = TableSearchOptions {
            search_term: "test".to_string(),
            case_sensitive: false,
            max_results: 10,
            ..Default::default()
        };

        let query = strategy.build_fallback_query(&ns, "users", &["name".to_string(), "email".to_string()], &options);

        assert!(query.contains("ILIKE"));
        assert!(query.contains("%test%"));
        assert!(query.contains("\"public\".\"users\""));
    }
}
