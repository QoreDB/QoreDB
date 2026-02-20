// SPDX-License-Identifier: BUSL-1.1

//! Safety validation for AI-generated queries.
//!
//! Reuses the existing `sql_safety` module for SQL drivers and provides
//! basic pattern-based checks for MongoDB.

use super::types::SafetyInfo;
use crate::engine::sql_safety;

/// Validate a generated query and return safety information.
///
/// For SQL drivers, delegates to the existing `sql_safety::analyze_sql`.
/// For MongoDB, does basic pattern matching on dangerous operations.
pub fn validate_generated_query(driver_id: &str, query: &str) -> SafetyInfo {
    match driver_id {
        "mongodb" => validate_mongo_query(query),
        "redis" => validate_redis_query(query),
        _ => validate_sql_query(driver_id, query),
    }
}

fn validate_sql_query(driver_id: &str, query: &str) -> SafetyInfo {
    match sql_safety::analyze_sql(driver_id, query) {
        Ok(analysis) => {
            let mut warnings = Vec::new();

            if analysis.is_mutation {
                warnings.push("This query modifies data.".to_string());
            }
            if analysis.is_dangerous {
                warnings.push(
                    "This query is potentially dangerous (DROP, TRUNCATE, or DELETE without WHERE)."
                        .to_string(),
                );
            }

            SafetyInfo {
                is_mutation: analysis.is_mutation,
                is_dangerous: analysis.is_dangerous,
                warnings,
            }
        }
        Err(_) => {
            // If parsing fails, be cautious
            SafetyInfo {
                is_mutation: false,
                is_dangerous: false,
                warnings: vec!["Could not parse generated query for safety analysis.".to_string()],
            }
        }
    }
}

fn validate_mongo_query(query: &str) -> SafetyInfo {
    let lower = query.to_lowercase();
    let mut is_mutation = false;
    let mut is_dangerous = false;
    let mut warnings = Vec::new();

    // Mutation patterns
    let mutation_patterns = [
        "insertone", "insertmany", "updateone", "updatemany",
        "deleteone", "deletemany", "replaceone", "bulkwrite",
        "findoneandupdate", "findoneandreplace", "findoneanddelete",
    ];
    for pattern in &mutation_patterns {
        if lower.contains(pattern) {
            is_mutation = true;
            break;
        }
    }

    // Dangerous patterns
    let dangerous_patterns = [
        "deletemany({})", "deletemany( {} )", "deletemany()",
        ".drop(", "dropdatabase", "dropcollection",
    ];
    for pattern in &dangerous_patterns {
        if lower.replace(' ', "").contains(&pattern.replace(' ', "")) {
            is_dangerous = true;
            break;
        }
    }

    if is_mutation {
        warnings.push("This command modifies data.".to_string());
    }
    if is_dangerous {
        warnings.push("This command is potentially dangerous (drop or mass delete).".to_string());
    }

    SafetyInfo {
        is_mutation,
        is_dangerous,
        warnings,
    }
}

fn validate_redis_query(query: &str) -> SafetyInfo {
    let lower = query.to_lowercase();
    let mut is_mutation = false;
    let mut is_dangerous = false;
    let mut warnings = Vec::new();

    // Mutation commands
    let mutation_cmds = [
        "set ", "del ", "hset ", "lpush ", "rpush ", "sadd ",
        "zadd ", "expire ", "rename ", "persist ", "incr ", "decr ",
    ];
    for cmd in &mutation_cmds {
        if lower.starts_with(cmd) || lower.contains(&format!("\n{}", cmd)) {
            is_mutation = true;
            break;
        }
    }

    // Dangerous commands
    if lower.contains("flushall") || lower.contains("flushdb") {
        is_dangerous = true;
    }

    if is_mutation {
        warnings.push("This command modifies data.".to_string());
    }
    if is_dangerous {
        warnings.push("This command is potentially dangerous (FLUSHALL/FLUSHDB).".to_string());
    }

    SafetyInfo {
        is_mutation,
        is_dangerous,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_select() {
        let info = validate_generated_query("postgres", "SELECT * FROM users WHERE id = 1");
        assert!(!info.is_mutation);
        assert!(!info.is_dangerous);
        assert!(info.warnings.is_empty());
    }

    #[test]
    fn test_mutation_update() {
        let info =
            validate_generated_query("postgres", "UPDATE users SET name = 'John' WHERE id = 1");
        assert!(info.is_mutation);
        assert!(!info.is_dangerous);
    }

    #[test]
    fn test_dangerous_drop() {
        let info = validate_generated_query("postgres", "DROP TABLE users");
        assert!(info.is_mutation);
        assert!(info.is_dangerous);
    }

    #[test]
    fn test_mongo_find() {
        let info = validate_generated_query("mongodb", "db.users.find({age: {$gt: 25}})");
        assert!(!info.is_mutation);
        assert!(!info.is_dangerous);
    }

    #[test]
    fn test_mongo_delete_many_empty() {
        let info = validate_generated_query("mongodb", "db.users.deleteMany({})");
        assert!(info.is_mutation);
        assert!(info.is_dangerous);
    }

    #[test]
    fn test_mongo_insert() {
        let info =
            validate_generated_query("mongodb", r#"db.users.insertOne({name: "Alice", age: 30})"#);
        assert!(info.is_mutation);
        assert!(!info.is_dangerous);
    }

    #[test]
    fn test_redis_get() {
        let info = validate_generated_query("redis", "GET user:1");
        assert!(!info.is_mutation);
        assert!(!info.is_dangerous);
    }

    #[test]
    fn test_redis_flushall() {
        let info = validate_generated_query("redis", "FLUSHALL");
        assert!(info.is_dangerous);
    }
}
