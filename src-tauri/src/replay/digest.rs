// SPDX-License-Identifier: BUSL-1.1

//! Canonical digest of a result set.
//!
//! Rows are hashed after sorting, because no engine guarantees an order
//! without `ORDER BY`: without the sort, a stable result would report a diff
//! on every run. Ignored columns are dropped before hashing, and each field is
//! length-prefixed so `["ab", "c"]` and `["a", "bc"]` cannot collide.

use sha2::{Digest, Sha256};
use std::collections::BinaryHeap;

use crate::engine::types::{QueryResult, Value};

pub struct DigestOutcome {
    pub digest: String,
    pub partial: bool,
}

fn encode_value(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| format!("{:?}", value))
}

pub fn compute_digest(
    result: &QueryResult,
    ignored_columns: &[String],
    max_rows: usize,
) -> DigestOutcome {
    let kept: Vec<usize> = result
        .columns
        .iter()
        .enumerate()
        .filter(|(_, col)| {
            !ignored_columns
                .iter()
                .any(|ignored| ignored.eq_ignore_ascii_case(col.name.as_str()))
        })
        .map(|(index, _)| index)
        .collect();

    // A max-heap of the smallest `max_rows` encodings seen so far: pushing then
    // popping the largest keeps the bound without ever holding the whole set.
    let mut smallest: BinaryHeap<String> = BinaryHeap::new();
    let mut total = 0usize;
    for row in &result.rows {
        total += 1;
        let mut buffer = String::new();
        for &index in &kept {
            let field = match row.values.get(index) {
                Some(value) => encode_value(value),
                None => "\u{0}missing".to_string(),
            };
            buffer.push_str(&field.len().to_string());
            buffer.push(':');
            buffer.push_str(&field);
        }

        if smallest.len() < max_rows {
            smallest.push(buffer);
        } else if smallest.peek().is_some_and(|largest| buffer < *largest) {
            smallest.pop();
            smallest.push(buffer);
        }
    }

    let partial = total > max_rows;
    let encoded: Vec<String> = smallest.into_sorted_vec();

    let mut hasher = Sha256::new();
    for &index in &kept {
        let name = result.columns[index].name.as_str();
        hasher.update(name.len().to_string().as_bytes());
        hasher.update(b":");
        hasher.update(name.as_bytes());
    }
    hasher.update(b"\n");
    for row in &encoded {
        hasher.update(row.len().to_string().as_bytes());
        hasher.update(b":");
        hasher.update(row.as_bytes());
    }

    DigestOutcome {
        digest: format!("sha256:{:x}", hasher.finalize()),
        partial,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::{ColumnInfo, Row};

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
        }
    }

    fn result(columns: &[&str], rows: Vec<Vec<Value>>) -> QueryResult {
        QueryResult {
            columns: columns.iter().map(|c| column(c)).collect(),
            rows: rows.into_iter().map(|values| Row { values }).collect(),
            affected_rows: None,
            execution_time_ms: 0.0,
        }
    }

    #[test]
    fn digest_is_stable_under_row_permutation() {
        let a = result(
            &["id", "name"],
            vec![
                vec![Value::Int(1), Value::Text("a".into())],
                vec![Value::Int(2), Value::Text("b".into())],
            ],
        );
        let b = result(
            &["id", "name"],
            vec![
                vec![Value::Int(2), Value::Text("b".into())],
                vec![Value::Int(1), Value::Text("a".into())],
            ],
        );
        assert_eq!(
            compute_digest(&a, &[], 1000).digest,
            compute_digest(&b, &[], 1000).digest
        );
    }

    #[test]
    fn digest_changes_when_a_value_changes() {
        let a = result(&["id"], vec![vec![Value::Int(1)]]);
        let b = result(&["id"], vec![vec![Value::Int(2)]]);
        assert_ne!(
            compute_digest(&a, &[], 1000).digest,
            compute_digest(&b, &[], 1000).digest
        );
    }

    #[test]
    fn ignored_columns_are_excluded() {
        let a = result(
            &["id", "updated_at"],
            vec![vec![Value::Int(1), Value::Text("2026-01-01".into())]],
        );
        let b = result(
            &["id", "updated_at"],
            vec![vec![Value::Int(1), Value::Text("2026-08-21".into())]],
        );
        let ignored = vec!["updated_at".to_string()];
        assert_ne!(
            compute_digest(&a, &[], 1000).digest,
            compute_digest(&b, &[], 1000).digest
        );
        assert_eq!(
            compute_digest(&a, &ignored, 1000).digest,
            compute_digest(&b, &ignored, 1000).digest
        );
    }

    #[test]
    fn ignored_columns_match_case_insensitively() {
        let value = result(
            &["Updated_At"],
            vec![vec![Value::Text("2026-01-01".into())]],
        );
        let other = result(
            &["Updated_At"],
            vec![vec![Value::Text("2026-08-21".into())]],
        );
        let ignored = vec!["updated_at".to_string()];
        assert_eq!(
            compute_digest(&value, &ignored, 1000).digest,
            compute_digest(&other, &ignored, 1000).digest
        );
    }

    #[test]
    fn column_rename_changes_the_digest() {
        let a = result(&["id"], vec![vec![Value::Int(1)]]);
        let b = result(&["identifier"], vec![vec![Value::Int(1)]]);
        assert_ne!(
            compute_digest(&a, &[], 1000).digest,
            compute_digest(&b, &[], 1000).digest
        );
    }

    #[test]
    fn bound_flags_partial_and_still_ignores_input_order() {
        let rows: Vec<Vec<Value>> = (0..10).map(|i| vec![Value::Int(i)]).collect();
        let mut reversed = rows.clone();
        reversed.reverse();

        let a = compute_digest(&result(&["id"], rows), &[], 5);
        let b = compute_digest(&result(&["id"], reversed), &[], 5);

        assert!(a.partial);
        assert!(b.partial);
        assert_eq!(a.digest, b.digest);
    }

    #[test]
    fn field_boundaries_cannot_collide() {
        let a = result(
            &["x", "y"],
            vec![vec![Value::Text("ab".into()), Value::Text("c".into())]],
        );
        let b = result(
            &["x", "y"],
            vec![vec![Value::Text("a".into()), Value::Text("bc".into())]],
        );
        assert_ne!(
            compute_digest(&a, &[], 1000).digest,
            compute_digest(&b, &[], 1000).digest
        );
    }
}
