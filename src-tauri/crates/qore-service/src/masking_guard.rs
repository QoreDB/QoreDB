// SPDX-License-Identifier: BUSL-1.1

//! Agents write their own queries, so on a connection with masking a masked
//! column may only appear as a bare projection: the output keeps the column
//! name and gets masked. Aliases, expressions, filters, sorts, set operations
//! and whole-row serialisation would carry the value out under another name,
//! or let the agent probe it.

use std::collections::HashSet;

use sqlparser::dialect::GenericDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, Tokenizer, Word};

use qore_core::masking::ConnectionMasking;

/// Full-row/document editors must never write display placeholders back to storage.
pub fn check_row_update(
    masking: &ConnectionMasking,
    table: &str,
    primary_key: &qore_core::RowData,
    data: &qore_core::RowData,
) -> Result<(), String> {
    fn contains_masked(
        masking: &ConnectionMasking,
        table: &str,
        value: &serde_json::Value,
    ) -> bool {
        match value {
            serde_json::Value::Object(fields) => fields.iter().any(|(key, value)| {
                masking.mode_for(Some(table), key).is_some()
                    || key
                        .split('.')
                        .any(|part| masking.mode_for(Some(table), part).is_some())
                    || contains_masked(masking, table, value)
            }),
            serde_json::Value::Array(values) => values
                .iter()
                .any(|value| contains_masked(masking, table, value)),
            _ => false,
        }
    }

    for row in [primary_key, data] {
        let object = serde_json::Value::Object(
            row.columns
                .iter()
                .map(|(key, value)| (key.clone(), value.to_json()))
                .collect(),
        );
        if contains_masked(masking, table, &object) {
            return Err("MASKED_FIELD_UPDATE".to_string());
        }
    }
    Ok(())
}

const ROW_SERIALIZERS: &[&str] = &[
    "row_to_json",
    "to_json",
    "to_jsonb",
    "json_agg",
    "jsonb_agg",
    "array_agg",
    "hstore",
    "row",
    "json_object",
    "json_build_object",
    "jsonb_build_object",
    "json_arrayagg",
    "json_objectagg",
    "object_construct",
    "to_json_string",
];

pub fn check_agent_query(
    driver_id: &str,
    query: &str,
    masking: &ConnectionMasking,
) -> Result<(), String> {
    let is_masked = |name: &str| masking.mode_for(None, name).is_some();

    if !is_sql_driver(driver_id) {
        return match query
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .find(|word| !word.is_empty() && is_masked(word))
        {
            Some(word) => Err(refusal(word)),
            None => Ok(()),
        };
    }

    let tokens: Vec<Token> = Tokenizer::new(&GenericDialect {}, query)
        .tokenize()
        .map_err(|_| {
            "Column masking is active on this connection and the query could not be analysed; \
             write it as a plain SELECT."
                .to_string()
        })?
        .into_iter()
        .filter(|token| !matches!(token, Token::Whitespace(_)))
        .collect();

    let mut depths = Vec::with_capacity(tokens.len());
    let mut depth = 0usize;
    for token in &tokens {
        if matches!(token, Token::RParen) {
            depth = depth.saturating_sub(1);
        }
        depths.push(depth);
        if matches!(token, Token::LParen) {
            depth += 1;
        }
    }

    for (index, token) in tokens.iter().enumerate() {
        let serializer = word(token).is_some_and(|w| {
            w.quote_style.is_none() && ROW_SERIALIZERS.contains(&w.value.to_lowercase().as_str())
        }) && matches!(tokens.get(index + 1), Some(Token::LParen));
        let star_in_call = depths[index] > 0
            && matches!(token, Token::Period)
            && matches!(tokens.get(index + 1), Some(Token::Mul));
        if serializer || star_in_call {
            return Err(
                "Column masking is active on this connection: whole-row serialisation \
                 (row_to_json, json_agg, t.* inside a function, ...) is refused."
                    .to_string(),
            );
        }
    }

    let (relation_positions, relation_names) = relations(&tokens, &depths);
    // Column lists rename by position, including columns hidden behind `*`.
    // Without source lineage we cannot safely mask their output names.
    for (index, token) in tokens.iter().enumerate() {
        if word(token).is_none() || !matches!(tokens.get(index + 1), Some(Token::LParen)) {
            continue;
        }
        let after_derived = index.checked_sub(1).is_some_and(|previous| {
            matches!(tokens[previous], Token::RParen)
                || (is_kw(&tokens[previous], "AS")
                    && previous
                        .checked_sub(1)
                        .is_some_and(|before_as| matches!(tokens[before_as], Token::RParen)))
        });
        let cte_columns = (index + 2..tokens.len())
            .find(|&end| depths[end] == depths[index])
            .is_some_and(|end| {
                matches!(tokens[end], Token::RParen)
                    && tokens.get(end + 1).is_some_and(|t| is_kw(t, "AS"))
                    && tokens.get(end + 2).is_some_and(|t| {
                        matches!(t, Token::LParen)
                            || (is_kw(t, "MATERIALIZED")
                                && matches!(tokens.get(end + 3), Some(Token::LParen)))
                            || (is_kw(t, "NOT")
                                && tokens
                                    .get(end + 3)
                                    .is_some_and(|t| is_kw(t, "MATERIALIZED"))
                                && matches!(tokens.get(end + 4), Some(Token::LParen)))
                    })
            });
        if relation_positions.contains(&index) || after_derived || cte_columns {
            return Err(
                "Column masking is active on this connection: column alias lists on CTEs \
                 and table references are refused. Keep the original column names."
                    .to_string(),
            );
        }
    }
    let masked: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(index, token)| {
            word(token).is_some_and(|w| is_masked(&w.value))
                && !relation_positions.contains(index)
                && !matches!(tokens.get(index + 1), Some(Token::Period))
        })
        .map(|(index, _)| index)
        .collect();

    let set_operation = tokens.iter().any(|token| {
        ["UNION", "INTERSECT", "EXCEPT", "MINUS"]
            .iter()
            .any(|k| is_kw(token, k))
    });
    if set_operation && !masked.is_empty() {
        return Err(refusal(&word(&tokens[masked[0]]).expect("word").value));
    }

    let mut allowed = HashSet::new();
    for item in projection_items(&tokens, &depths) {
        let Some(column) = bare_column(&tokens, &item) else {
            continue;
        };
        let single = item.len() == 1
            || (item.len() == 3 && is_kw(&tokens[item[1]], "AS"))
            || item.len() == 2;
        if single {
            let name = word(&tokens[column]).expect("word").value.to_lowercase();
            if relation_names.contains(&name) {
                return Err(format!(
                    "Column masking is active on this connection: selecting the whole row `{name}` \
                     as a value is refused. Select its columns instead."
                ));
            }
        }
        allowed.insert(column);
        // `email AS email`: the alias repeats the column name, so it is allowed too.
        if let Some(&alias) = item.last() {
            allowed.insert(alias);
        }
    }

    match masked.iter().find(|index| !allowed.contains(*index)) {
        Some(index) => Err(refusal(&word(&tokens[*index]).expect("word").value)),
        None => Ok(()),
    }
}

fn is_sql_driver(driver_id: &str) -> bool {
    !matches!(
        driver_id.to_ascii_lowercase().as_str(),
        "mongodb"
            | "documentdb"
            | "redis"
            | "valkey"
            | "dragonfly"
            | "keydb"
            | "garnet"
            | "elasticsearch"
            | "opensearch"
    )
}

fn refusal(column: &str) -> String {
    format!(
        "Column masking is active on this connection: `{column}` may only be selected as a plain \
         column (no alias, expression, filter, sort, join condition or set operation). Its values \
         come back masked."
    )
}

fn word(token: &Token) -> Option<&Word> {
    match token {
        Token::Word(word) => Some(word),
        _ => None,
    }
}

fn is_kw(token: &Token, keyword: &str) -> bool {
    word(token).is_some_and(|w| w.quote_style.is_none() && w.value.eq_ignore_ascii_case(keyword))
}

/// Table references after FROM/JOIN (with their schema qualifiers and
/// aliases): positions to skip, and names a bare projection must not use.
fn relations(tokens: &[Token], depths: &[usize]) -> (HashSet<usize>, HashSet<String>) {
    let mut positions = HashSet::new();
    let mut names = HashSet::new();
    let mut index = 0;
    while index < tokens.len() {
        if !(is_kw(&tokens[index], "FROM") || is_kw(&tokens[index], "JOIN")) {
            index += 1;
            continue;
        }
        let clause_depth = depths[index];
        index += 1;
        loop {
            if !matches!(tokens.get(index), Some(Token::Word(_))) {
                break;
            }
            while let Some(Token::Word(w)) = tokens.get(index) {
                positions.insert(index);
                if !matches!(tokens.get(index + 1), Some(Token::Period)) {
                    names.insert(w.value.to_lowercase());
                    index += 1;
                    break;
                }
                index += 2;
            }
            if tokens.get(index).is_some_and(|t| is_kw(t, "AS")) {
                index += 1;
            }
            if let Some(Token::Word(alias)) = tokens.get(index)
                && alias.keyword == Keyword::NoKeyword
            {
                positions.insert(index);
                names.insert(alias.value.to_lowercase());
                index += 1;
            }
            if matches!(tokens.get(index), Some(Token::Comma)) && depths[index] == clause_depth {
                index += 1;
                continue;
            }
            break;
        }
    }
    (positions, names)
}

/// Token positions of each SELECT list item, at every nesting level.
fn projection_items(tokens: &[Token], depths: &[usize]) -> Vec<Vec<usize>> {
    let mut items = Vec::new();
    for (start, token) in tokens.iter().enumerate() {
        if !is_kw(token, "SELECT") {
            continue;
        }
        let depth = depths[start];
        let mut current = Vec::new();
        let mut index = start + 1;
        while index < tokens.len() {
            let token = &tokens[index];
            if depths[index] < depth
                || (depths[index] == depth
                    && [
                        "FROM", "WHERE", "GROUP", "ORDER", "LIMIT", "HAVING", "INTO", "UNION",
                    ]
                    .iter()
                    .any(|k| is_kw(token, k)))
            {
                break;
            }
            if depths[index] == depth && matches!(token, Token::Comma) {
                items.push(std::mem::take(&mut current));
            } else if !(current.is_empty()
                && depths[index] == depth
                && (is_kw(token, "DISTINCT") || is_kw(token, "ALL")))
            {
                current.push(index);
            }
            index += 1;
        }
        if !current.is_empty() {
            items.push(current);
        }
    }
    items
}

/// `col`, `t.col`, `s.t.col`, optionally followed by `AS col` or `col` with
/// the same name. Returns the position of the column word.
fn bare_column(tokens: &[Token], item: &[usize]) -> Option<usize> {
    let mut end = item.len();
    let mut alias = None;
    if end >= 3 && is_kw(&tokens[item[end - 2]], "AS") {
        alias = word(&tokens[item[end - 1]]);
        end -= 2;
    } else if end >= 2
        && matches!(tokens[item[end - 2]], Token::Word(_))
        && matches!(tokens[item[end - 1]], Token::Word(_))
    {
        alias = word(&tokens[item[end - 1]]);
        end -= 1;
    }
    let path = &item[..end];
    if path.is_empty() || path.len().is_multiple_of(2) {
        return None;
    }
    for (offset, position) in path.iter().enumerate() {
        let expected_word = offset % 2 == 0;
        match &tokens[*position] {
            Token::Word(_) if expected_word => {}
            Token::Period if !expected_word => {}
            _ => return None,
        }
    }
    let column = *path.last()?;
    let name = &word(&tokens[column])?.value;
    match alias {
        Some(alias) if !alias.value.eq_ignore_ascii_case(name) => None,
        _ => Some(column),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qore_core::masking::{MaskMode, MaskingRule};

    fn masking() -> ConnectionMasking {
        ConnectionMasking {
            rules: vec![MaskingRule {
                table: "users".to_string(),
                column: "email".to_string(),
                mode: MaskMode::Partial,
            }],
            mask_detected_columns: false,
        }
    }

    fn check(query: &str) -> Result<(), String> {
        check_agent_query("postgres", query, &masking())
    }

    #[test]
    fn bare_projections_and_unrelated_queries_pass() {
        for query in [
            "SELECT id, email FROM users",
            "SELECT u.email, u.id FROM public.users AS u WHERE u.id = 1",
            "SELECT DISTINCT email FROM users",
            "SELECT email AS email FROM users",
            "SELECT * FROM users ORDER BY id LIMIT 10",
            "SELECT count(*) FROM users",
            "SELECT e.email FROM (SELECT id, email FROM users) e",
            "SELECT 'email' AS label FROM users",
        ] {
            assert!(check(query).is_ok(), "{query}: {:?}", check(query));
        }
    }

    #[test]
    fn masked_columns_outside_a_bare_projection_are_refused() {
        for query in [
            "SELECT email AS contact FROM users",
            "SELECT lower(email) FROM users",
            "SELECT id FROM users WHERE email LIKE 'a%'",
            "SELECT id FROM users ORDER BY email",
            "SELECT id FROM users u JOIN leads l ON l.email = u.email",
            "SELECT name FROM customers UNION SELECT email FROM users",
            "SELECT e FROM (SELECT email AS e FROM users) x",
        ] {
            assert!(check(query).is_err(), "{query} should be refused");
        }
    }

    #[test]
    fn whole_row_serialisation_is_refused() {
        for query in [
            "SELECT row_to_json(u) FROM users u",
            "SELECT json_agg(u.*) FROM users u",
            "SELECT u FROM users u",
            "SELECT users FROM users",
        ] {
            assert!(check(query).is_err(), "{query} should be refused");
        }
    }

    #[test]
    fn positional_column_aliases_cannot_bypass_masking() {
        for query in [
            "WITH c(leaked) AS (SELECT email FROM users) SELECT leaked FROM c",
            "WITH RECURSIVE c(leaked) AS (SELECT email FROM users) SELECT * FROM c",
            "WITH c(leaked) AS MATERIALIZED (SELECT email FROM users) SELECT * FROM c",
            "WITH c(leaked) AS NOT MATERIALIZED (SELECT email FROM users) SELECT * FROM c",
            "WITH a AS (SELECT 1), c(id, leaked) AS (SELECT * FROM users) SELECT * FROM c",
            "SELECT leaked FROM (SELECT email FROM users) AS c(leaked)",
            "SELECT * FROM (SELECT * FROM users) c(id, leaked)",
            "SELECT * FROM users AS u(id, leaked)",
            "SELECT * FROM users u(id, leaked)",
        ] {
            assert!(check(query).is_err(), "{query} should be refused");
        }
        assert!(check("WITH c AS (SELECT email FROM users) SELECT email FROM c").is_ok());
        assert!(check("SELECT count(*) AS total FROM users").is_ok());
        assert!(check("SELECT count(*) AS materialized FROM users").is_ok());
    }

    #[test]
    fn document_stores_refuse_any_mention_of_a_masked_key() {
        let rules = masking();
        assert!(check_agent_query("mongodb", r#"db.users.find({})"#, &rules).is_ok());
        assert!(
            check_agent_query(
                "mongodb",
                r#"db.users.aggregate([{"$project": {"e": "$email"}}])"#,
                &rules
            )
            .is_err()
        );
    }

    #[test]
    fn detection_checks_column_names_but_not_table_names() {
        let detected = ConnectionMasking {
            rules: Vec::new(),
            mask_detected_columns: true,
        };
        assert!(check_agent_query("postgres", "SELECT id FROM email_log", &detected).is_ok());
        assert!(
            check_agent_query(
                "postgres",
                "SELECT id FROM users WHERE phone = '1'",
                &detected
            )
            .is_err()
        );
    }

    #[test]
    fn updates_reject_masked_fields_and_keys_including_nested_documents() {
        use qore_core::{RowData, Value};
        let config = masking();
        let pk = RowData::new().with_column("id", Value::Int(1));
        for data in [
            RowData::new().with_column("email", Value::Text("••••••".into())),
            RowData::new().with_column("EMAIL", Value::Null),
            RowData::new().with_column("profile.email", Value::Text("changed".into())),
            RowData::new().with_column(
                "profile",
                Value::Json(serde_json::json!({"email": "••••••", "city": "Lyon"})),
            ),
            RowData::new().with_column(
                "contacts",
                Value::Json(serde_json::json!([{ "email": "••••••" }])),
            ),
        ] {
            assert!(check_row_update(&config, "public.users", &pk, &data).is_err());
            assert!(check_row_update(&config, "users", &data, &pk).is_err());
            assert!(check_row_update(&config, "other_table", &pk, &data).is_ok());
        }
        assert!(
            check_row_update(
                &config,
                "users",
                &pk,
                &RowData::new().with_column("city", Value::Text("Lyon".into()))
            )
            .is_ok()
        );
        let detected = ConnectionMasking {
            rules: vec![],
            mask_detected_columns: true,
        };
        assert!(
            check_row_update(
                &detected,
                "users",
                &pk,
                &RowData::new().with_column("password", Value::Null)
            )
            .is_err()
        );
    }
}
