// SPDX-License-Identifier: Apache-2.0

//! Opaque keyset cursors.
//!
//! A cursor carries the ordering key values of the row a page stopped on. It
//! never carries SQL: the driver rebuilds the predicate from the ordering it
//! decided for the request, and the cursor only supplies bound values. A
//! cursor minted for a different ordering is rejected rather than
//! reinterpreted, because reusing its values against other columns would
//! silently return the wrong rows.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use crate::error::{EngineError, EngineResult};
use crate::types::Value;

/// Encoded cursors above this length are refused before any parsing.
const MAX_ENCODED_LEN: usize = 4096;
/// A keyset beyond this many columns is not something the product produces.
const MAX_KEYS: usize = 8;
/// Bound on a single text key value, so a cursor cannot smuggle a payload.
const MAX_TEXT_LEN: usize = 1024;

/// One column of the ordering a cursor was minted for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorKey {
    pub column: String,
    pub descending: bool,
}

/// The boundary row of a page, expressed in ordering-key values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cursor {
    /// Ordering this cursor belongs to. Compared against the request's own
    /// ordering on decode; it is never used to build SQL.
    pub keys: Vec<CursorKey>,
    /// Key values of the boundary row, positionally matching `keys`.
    pub values: Vec<Value>,
}

impl Cursor {
    pub fn new(keys: Vec<CursorKey>, values: Vec<Value>) -> Self {
        Self { keys, values }
    }

    pub fn encode(&self) -> EngineResult<String> {
        let json = serde_json::to_vec(self)
            .map_err(|e| EngineError::internal(format!("Cursor encoding failed: {e}")))?;
        Ok(URL_SAFE_NO_PAD.encode(json))
    }

    /// Decodes and validates `encoded` against the ordering `expected`.
    ///
    /// Every failure is the same class of event for the caller: the cursor
    /// cannot be honoured, so pagination has to restart from the first page.
    pub fn decode(encoded: &str, expected: &[CursorKey]) -> EngineResult<Self> {
        if encoded.len() > MAX_ENCODED_LEN {
            return Err(EngineError::validation("Cursor is too long"));
        }
        let raw = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| EngineError::validation("Cursor is not valid base64url"))?;
        let cursor: Self = serde_json::from_slice(&raw)
            .map_err(|_| EngineError::validation("Cursor payload is malformed"))?;

        if cursor.keys.len() != cursor.values.len() {
            return Err(EngineError::validation("Cursor keys and values disagree"));
        }
        if cursor.keys.is_empty() || cursor.keys.len() > MAX_KEYS {
            return Err(EngineError::validation("Cursor key count is out of range"));
        }
        for value in &cursor.values {
            if let Value::Text(text) = value {
                if text.len() > MAX_TEXT_LEN {
                    return Err(EngineError::validation("Cursor value is too long"));
                }
            }
        }
        // A cursor from another ordering would point at the right values on the
        // wrong columns, which reads as data loss rather than as an error.
        if cursor.keys != expected {
            return Err(EngineError::validation(
                "Cursor does not match the current sort order",
            ));
        }
        Ok(cursor)
    }
}

/// The ordering a keyset walk follows, and the SQL fragments it produces.
///
/// One implementation for every SQL driver: the lexicographic predicate is the
/// part that is easy to get subtly wrong, and a subtle mistake here skips rows
/// rather than failing.
#[derive(Debug, Clone)]
pub struct KeysetPlan {
    keys: Vec<CursorKey>,
}

impl KeysetPlan {
    /// Ordering for a request: the requested sort leads, the caller's unique
    /// key breaks ties. Returns `None` without a unique key — there is then no
    /// total order, and a keyset without one repeats or skips rows.
    pub fn new(
        sort_column: Option<&str>,
        descending: bool,
        unique_key: Option<&[String]>,
    ) -> Option<Self> {
        let unique = unique_key.filter(|cols| !cols.is_empty())?;
        let mut keys: Vec<CursorKey> = Vec::new();
        if let Some(column) = sort_column {
            keys.push(CursorKey {
                column: column.to_string(),
                descending,
            });
        }
        for column in unique {
            if !keys.iter().any(|key| &key.column == column) {
                keys.push(CursorKey {
                    column: column.clone(),
                    descending,
                });
            }
        }
        (!keys.is_empty()).then_some(Self { keys })
    }

    pub fn keys(&self) -> &[CursorKey] {
        &self.keys
    }

    pub fn order_by(&self, quote: impl Fn(&str) -> String) -> String {
        self.keys
            .iter()
            .map(|key| {
                format!(
                    "{} {}",
                    quote(&key.column),
                    if key.descending { "DESC" } else { "ASC" }
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Expanded lexicographic comparison against the cursor boundary.
    ///
    /// Written as `k1 > v1 OR (k1 = v1 AND k2 > v2)` rather than the row
    /// constructor `(k1, k2) > (v1, v2)`: the latter reads better and indexes
    /// better, but cannot mix ASC and DESC keys.
    ///
    /// `bind` renders the placeholder for the i-th cursor value, 0-based.
    pub fn predicate(
        &self,
        quote: impl Fn(&str) -> String,
        bind: impl Fn(usize) -> String,
    ) -> String {
        let branches: Vec<String> = self
            .keys
            .iter()
            .enumerate()
            .map(|(index, key)| {
                let mut terms: Vec<String> = self.keys[..index]
                    .iter()
                    .enumerate()
                    .map(|(prior, earlier)| format!("{} = {}", quote(&earlier.column), bind(prior)))
                    .collect();
                terms.push(format!(
                    "{} {} {}",
                    quote(&key.column),
                    if key.descending { "<" } else { ">" },
                    bind(index)
                ));
                format!("({})", terms.join(" AND "))
            })
            .collect();
        format!("({})", branches.join(" OR "))
    }

    /// Cursor values in the order a positional placeholder consumes them.
    ///
    /// `predicate` names each value by index, which a numbered placeholder
    /// (`$2`, `@p2`) repeats at no cost. A positional one (`?`) cannot: every
    /// occurrence consumes the next bound value, and the expanded predicate
    /// mentions each earlier key again in every later branch. Binding the
    /// cursor values as they come therefore leaves the statement short of
    /// parameters — a plain error on DuckDB and MySQL, and silently wrong rows
    /// on SQLite, which reads a missing parameter as NULL.
    pub fn positional_values(&self, values: &[Value]) -> Vec<Value> {
        (0..self.keys.len())
            .flat_map(|index| values.iter().take(index + 1).cloned())
            .collect()
    }

    /// Decodes an incoming cursor against this ordering.
    pub fn decode(&self, encoded: &str) -> EngineResult<Cursor> {
        Cursor::decode(encoded, &self.keys)
    }

    /// Boundary values of the last row of `result`, in key order.
    ///
    /// `None` when a key column is missing from the projection: without its
    /// value the boundary would be wrong, and a wrong boundary skips rows
    /// silently. The caller then reports offset rather than paginate wrongly.
    pub fn boundary(&self, columns: &[String], last_row: &[Value]) -> Option<Vec<Value>> {
        self.keys
            .iter()
            .map(|key| {
                let index = columns.iter().position(|name| name == &key.column)?;
                last_row.get(index).cloned()
            })
            .collect()
    }

    pub fn mint(&self, columns: &[String], last_row: &[Value]) -> Option<String> {
        let values = self.boundary(columns, last_row)?;
        Cursor::new(self.keys.clone(), values).encode().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> Vec<CursorKey> {
        vec![
            CursorKey {
                column: "created_at".into(),
                descending: true,
            },
            CursorKey {
                column: "id".into(),
                descending: false,
            },
        ]
    }

    fn plan() -> KeysetPlan {
        KeysetPlan::new(Some("created_at"), true, Some(&["id".to_string()])).unwrap()
    }

    #[test]
    fn the_sort_leads_and_the_unique_key_breaks_ties() {
        let keys = plan().keys().to_vec();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].column, "created_at");
        assert_eq!(keys[1].column, "id");
        assert!(keys.iter().all(|key| key.descending));
    }

    #[test]
    fn a_sort_on_the_unique_key_is_not_repeated_as_its_own_tie_breaker() {
        let plan = KeysetPlan::new(Some("id"), false, Some(&["id".to_string()])).unwrap();
        assert_eq!(plan.keys().len(), 1);
    }

    #[test]
    fn without_a_unique_key_there_is_no_plan_at_all() {
        assert!(KeysetPlan::new(Some("name"), false, None).is_none());
        assert!(KeysetPlan::new(Some("name"), false, Some(&[])).is_none());
    }

    #[test]
    fn the_predicate_is_lexicographic_and_follows_each_key_direction() {
        let mixed = KeysetPlan {
            keys: vec![
                CursorKey {
                    column: "a".into(),
                    descending: true,
                },
                CursorKey {
                    column: "b".into(),
                    descending: false,
                },
            ],
        };
        let sql = mixed.predicate(|col| format!("\"{col}\""), |i| format!("${}", i + 1));
        assert_eq!(sql, "((\"a\" < $1) OR (\"a\" = $1 AND \"b\" > $2))");
        assert_eq!(
            mixed.order_by(|col| format!("\"{col}\"")),
            "\"a\" DESC, \"b\" ASC"
        );
    }

    #[test]
    fn the_boundary_comes_from_the_key_columns_wherever_they_sit() {
        let columns = vec![
            "id".to_string(),
            "name".to_string(),
            "created_at".to_string(),
        ];
        let row = vec![
            Value::Int(7),
            Value::Text("x".into()),
            Value::Text("2026-01-01".into()),
        ];
        let values = plan().boundary(&columns, &row).unwrap();
        assert_eq!(
            serde_json::to_value(&values).unwrap(),
            serde_json::json!(["2026-01-01", 7])
        );
    }

    #[test]
    fn a_key_missing_from_the_projection_yields_no_cursor() {
        let columns = vec!["name".to_string()];
        let row = vec![Value::Text("x".into())];
        assert!(plan().boundary(&columns, &row).is_none());
        assert!(plan().mint(&columns, &row).is_none());
    }

    #[test]
    fn round_trips_through_the_encoded_form() {
        let cursor = Cursor::new(
            keys(),
            vec![Value::Text("2026-01-01".into()), Value::Int(7)],
        );
        let decoded = Cursor::decode(&cursor.encode().unwrap(), &keys()).unwrap();
        assert_eq!(decoded.keys, cursor.keys);
        assert_eq!(
            serde_json::to_value(&decoded.values).unwrap(),
            serde_json::to_value(&cursor.values).unwrap()
        );
    }

    #[test]
    fn refuses_a_cursor_minted_for_another_ordering() {
        let cursor = Cursor::new(
            keys(),
            vec![Value::Text("2026-01-01".into()), Value::Int(7)],
        );
        let encoded = cursor.encode().unwrap();

        let other = vec![CursorKey {
            column: "name".into(),
            descending: false,
        }];
        assert!(Cursor::decode(&encoded, &other).is_err());

        // Same columns, reversed direction: the boundary would be on the wrong
        // side of the comparison.
        let flipped: Vec<CursorKey> = keys()
            .into_iter()
            .map(|key| CursorKey {
                descending: !key.descending,
                ..key
            })
            .collect();
        assert!(Cursor::decode(&encoded, &flipped).is_err());
    }

    #[test]
    fn refuses_malformed_bounded_and_inconsistent_payloads() {
        assert!(Cursor::decode("not base64!!", &keys()).is_err());
        assert!(Cursor::decode(&"a".repeat(MAX_ENCODED_LEN + 1), &keys()).is_err());
        assert!(Cursor::decode(&URL_SAFE_NO_PAD.encode("{}"), &keys()).is_err());

        let mismatched = Cursor {
            keys: keys(),
            values: vec![Value::Int(1)],
        };
        assert!(Cursor::decode(&mismatched.encode().unwrap(), &keys()).is_err());

        let oversized = Cursor::new(
            vec![CursorKey {
                column: "id".into(),
                descending: false,
            }],
            vec![Value::Text("x".repeat(MAX_TEXT_LEN + 1))],
        );
        let expected = vec![CursorKey {
            column: "id".into(),
            descending: false,
        }];
        assert!(Cursor::decode(&oversized.encode().unwrap(), &expected).is_err());
    }

    #[test]
    fn carries_no_sql_and_survives_a_hostile_column_name() {
        // The column name travels only to be compared with the expected
        // ordering; a driver never interpolates it.
        let hostile = vec![CursorKey {
            column: "id\"; DROP TABLE users; --".into(),
            descending: false,
        }];
        let cursor = Cursor::new(hostile.clone(), vec![Value::Int(1)]);
        let decoded = Cursor::decode(&cursor.encode().unwrap(), &hostile).unwrap();
        assert_eq!(decoded.keys, hostile);
        assert!(Cursor::decode(&cursor.encode().unwrap(), &[]).is_err());
    }

    /// The count and order must match what `predicate` emits, or a positional
    /// dialect binds the wrong values — silently, on an engine that reads a
    /// missing parameter as NULL.
    #[test]
    fn positional_values_follow_the_placeholders() {
        let plan = KeysetPlan::new(
            Some("bucket"),
            false,
            Some(&["id".to_string(), "shard".to_string()]),
        )
        .expect("a unique key yields a plan");

        let values = vec![Value::Text("a".into()), Value::Int(7), Value::Int(3)];
        let bound = plan.positional_values(&values);

        let placeholders = plan
            .predicate(|col| col.to_string(), |_| "?".to_string())
            .matches('?')
            .count();
        assert_eq!(bound.len(), placeholders);

        let rendered: Vec<String> = bound.iter().map(|v| format!("{v:?}")).collect();
        assert_eq!(
            rendered,
            vec![
                format!("{:?}", values[0]),
                format!("{:?}", values[0]),
                format!("{:?}", values[1]),
                format!("{:?}", values[0]),
                format!("{:?}", values[1]),
                format!("{:?}", values[2]),
            ]
        );
    }

    #[test]
    fn a_single_key_binds_once() {
        let plan = KeysetPlan::new(None, false, Some(&["id".to_string()])).expect("plan");
        let bound = plan.positional_values(&[Value::Int(42)]);
        assert_eq!(bound.len(), 1);
    }
}
