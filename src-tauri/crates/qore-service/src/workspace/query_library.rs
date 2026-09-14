// SPDX-License-Identifier: Apache-2.0

//! Query library of a file-based workspace (`.qoredb/queries/library.json`):
//! written by the desktop app, read by the agent surfaces.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Folders and items stay untyped so the app round-trips the fields it owns.
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceQueryLibrary {
    pub version: u32,
    pub folders: Vec<serde_json::Value>,
    pub items: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedQuery {
    pub id: String,
    pub title: String,
    pub query: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub variables: HashMap<String, QueryVariable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryVariable {
    #[serde(rename = "type")]
    pub kind: VariableKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VariableKind {
    Text,
    Number,
    Date,
    Select,
    #[serde(other)]
    Unknown,
}

pub fn read(qoredb_path: &Path) -> Result<WorkspaceQueryLibrary, String> {
    let path = qoredb_path.join("queries").join("library.json");
    if !path.exists() {
        return Ok(WorkspaceQueryLibrary {
            version: 1,
            folders: Vec::new(),
            items: Vec::new(),
        });
    }
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read library: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("Invalid library format: {e}"))
}

pub fn saved_queries(library: &WorkspaceQueryLibrary) -> Vec<SavedQuery> {
    library
        .items
        .iter()
        .filter_map(|item| serde_json::from_value(item.clone()).ok())
        .collect()
}

/// Same placeholders as the app (`{{name}}`, `$name`), resolved in one pass so
/// a substituted value is never scanned again. Only declared variables are
/// replaced: `$1` or Mongo's `$match` stay as written. Values become literals,
/// never identifiers.
pub fn substitute_variables(
    query: &str,
    definitions: &HashMap<String, QueryVariable>,
    values: &HashMap<String, String>,
) -> Result<String, String> {
    let unknown: BTreeSet<&str> = values
        .keys()
        .map(String::as_str)
        .filter(|name| !definitions.contains_key(*name))
        .collect();
    if !unknown.is_empty() {
        return Err(format!("Unknown variable(s): {}", name_list(&unknown)));
    }

    static PLACEHOLDER: OnceLock<Regex> = OnceLock::new();
    let placeholder = PLACEHOLDER
        .get_or_init(|| Regex::new(r"\{\{(\w+)\}\}|\$(\w+)").expect("valid placeholder regex"));

    let mut out = String::with_capacity(query.len());
    let mut last = 0;
    let mut missing = BTreeSet::new();
    let mut invalid = BTreeSet::new();
    for caps in placeholder.captures_iter(query) {
        let whole = caps.get(0).expect("group 0 always matches");
        let (name, dollar) = match caps.get(1) {
            Some(name) => (name.as_str(), false),
            None => (caps.get(2).expect("one alternative matches").as_str(), true),
        };
        if dollar && query[..whole.start()].ends_with('$') {
            continue;
        }
        let Some(definition) = definitions.get(name) else {
            continue;
        };
        let Some(raw) = values.get(name).or(definition.default_value.as_ref()) else {
            missing.insert(name);
            continue;
        };
        match literal(definition.kind, raw) {
            Some(value) => {
                out.push_str(&query[last..whole.start()]);
                out.push_str(&value);
                last = whole.end();
            }
            None => {
                invalid.insert(name);
            }
        }
    }

    if !missing.is_empty() {
        return Err(format!(
            "Missing value for variable(s): {}",
            name_list(&missing)
        ));
    }
    if !invalid.is_empty() {
        return Err(format!(
            "Invalid value for variable(s): {} (number expects a finite number, date expects \
             YYYY-MM-DD or an ISO-8601 timestamp)",
            name_list(&invalid)
        ));
    }
    out.push_str(&query[last..]);
    Ok(out)
}

fn name_list(names: &BTreeSet<&str>) -> String {
    names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn literal(kind: VariableKind, raw: &str) -> Option<String> {
    match kind {
        VariableKind::Number => raw
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(|n| n.to_string()),
        VariableKind::Date => {
            static DATE: OnceLock<Regex> = OnceLock::new();
            let date = DATE.get_or_init(|| {
                Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}([T ][0-9:.+\-Z]+)?$")
                    .expect("valid date regex")
            });
            let trimmed = raw.trim();
            date.is_match(trimmed).then(|| sql_quote(trimmed))
        }
        VariableKind::Text | VariableKind::Select => Some(sql_quote(raw)),
        VariableKind::Unknown => None,
    }
}

fn sql_quote(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|c| !matches!(c, '\0' | '\r' | '\n' | '\u{2028}' | '\u{2029}'))
        .collect();
    format!("'{}'", cleaned.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variable(kind: VariableKind, default: Option<&str>) -> QueryVariable {
        QueryVariable {
            kind,
            default_value: default.map(str::to_string),
            description: None,
            options: Vec::new(),
        }
    }

    fn definitions(entries: &[(&str, QueryVariable)]) -> HashMap<String, QueryVariable> {
        entries
            .iter()
            .map(|(name, v)| (name.to_string(), v.clone()))
            .collect()
    }

    fn values(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(name, v)| (name.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn substitutes_both_placeholder_styles_as_literals() {
        let defs = definitions(&[
            ("city", variable(VariableKind::Text, None)),
            ("limit", variable(VariableKind::Number, Some("10"))),
        ]);
        let sql = substitute_variables(
            "SELECT * FROM users WHERE city = {{city}} LIMIT $limit",
            &defs,
            &values(&[("city", "O'Brien\nville")]),
        )
        .unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM users WHERE city = 'O''Brienville' LIMIT 10"
        );
    }

    #[test]
    fn leaves_undeclared_and_dollar_quoted_tokens_alone() {
        let defs = definitions(&[("body", variable(VariableKind::Text, Some("x")))]);
        let sql = "SELECT $1, $$body$$, '$match'";
        assert_eq!(
            substitute_variables(sql, &defs, &HashMap::new()).unwrap(),
            sql
        );
    }

    #[test]
    fn substituted_values_are_not_scanned_again() {
        let defs = definitions(&[
            ("a", variable(VariableKind::Text, None)),
            ("b", variable(VariableKind::Number, None)),
        ]);
        let sql = substitute_variables("{{a}} {{b}}", &defs, &values(&[("a", "$b"), ("b", "2")]))
            .unwrap();
        assert_eq!(sql, "'$b' 2");
    }

    #[test]
    fn rejects_missing_invalid_and_unknown_values() {
        let defs = definitions(&[
            ("n", variable(VariableKind::Number, None)),
            ("day", variable(VariableKind::Date, None)),
        ]);

        let missing = substitute_variables("SELECT {{n}}", &defs, &HashMap::new()).unwrap_err();
        assert!(missing.contains("`n`"), "{missing}");

        let invalid = substitute_variables(
            "SELECT {{n}}, {{day}}",
            &defs,
            &values(&[("n", "1; DROP TABLE x"), ("day", "2026-13")]),
        )
        .unwrap_err();
        assert!(
            invalid.contains("`n`") && invalid.contains("`day`"),
            "{invalid}"
        );

        let unknown =
            substitute_variables("SELECT 1", &defs, &values(&[("nope", "1")])).unwrap_err();
        assert!(unknown.contains("`nope`"), "{unknown}");

        assert_eq!(
            substitute_variables(
                "WHERE ts > {{day}}",
                &defs,
                &values(&[("day", "2026-05-16")])
            )
            .unwrap(),
            "WHERE ts > '2026-05-16'"
        );
    }

    #[test]
    fn missing_library_reads_as_empty_and_malformed_items_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read(dir.path()).unwrap().items.is_empty());

        std::fs::create_dir_all(dir.path().join("queries")).unwrap();
        std::fs::write(
            dir.path().join("queries").join("library.json"),
            r#"{"version":1,"folders":[],"items":[
                {"id":"ql_1","title":"Users by city","query":"SELECT * FROM users WHERE city = {{city}}",
                 "folderId":null,"tags":["users"],"isFavorite":false,
                 "variables":{"city":{"name":"city","type":"text","defaultValue":"Paris"}},
                 "createdAt":1,"updatedAt":1},
                {"id":"broken"}
            ]}"#,
        )
        .unwrap();

        let queries = saved_queries(&read(dir.path()).unwrap());
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0].id, "ql_1");
        assert_eq!(
            queries[0].variables["city"].default_value.as_deref(),
            Some("Paris")
        );
    }
}
