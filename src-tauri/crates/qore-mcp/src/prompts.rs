// SPDX-License-Identifier: Apache-2.0

use rmcp::ErrorData as McpError;
use rmcp::model::{
    GetPromptRequestParams, GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage,
    PromptMessageRole,
};

pub fn definitions() -> Vec<Prompt> {
    vec![
        Prompt::new(
            "audit_table",
            Some("Audit one table: structure, indexes, volume and sensitive columns"),
            Some(vec![
                required("connection_id", "ID of the exposed connection"),
                required("database", "Database or namespace holding the table"),
                optional("schema", "Schema, for engines that use one"),
                required("table", "Table or collection name"),
            ]),
        ),
        Prompt::new(
            "explain_slow_query",
            Some("Read the execution plan of a query and suggest improvements"),
            Some(vec![
                required("connection_id", "ID of the exposed connection"),
                required("database", "Database or namespace the query targets"),
                required("query", "The slow read-only query"),
            ]),
        ),
        Prompt::new(
            "document_schema",
            Some("Write Markdown documentation for every table of a namespace"),
            Some(vec![
                required("connection_id", "ID of the exposed connection"),
                required("database", "Database or namespace to document"),
                optional("schema", "Schema, for engines that use one"),
            ]),
        ),
    ]
}

pub fn render(request: &GetPromptRequestParams) -> Result<GetPromptResult, McpError> {
    let args = request.arguments.as_ref();
    let text = match request.name.as_str() {
        "audit_table" => audit_table(args)?,
        "explain_slow_query" => explain_slow_query(args)?,
        "document_schema" => document_schema(args)?,
        other => {
            return Err(McpError::invalid_params(
                format!("Unknown prompt: {other}"),
                None,
            ));
        }
    };
    Ok(GetPromptResult::new(vec![PromptMessage::new_text(
        PromptMessageRole::User,
        text,
    )]))
}

fn required(name: &str, description: &str) -> PromptArgument {
    PromptArgument::new(name)
        .with_description(description)
        .with_required(true)
}

fn optional(name: &str, description: &str) -> PromptArgument {
    PromptArgument::new(name).with_description(description)
}

fn arg<'a>(args: Option<&'a JsonObject>, key: &str) -> Result<&'a str, McpError> {
    args.and_then(|a| a.get(key))
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| McpError::invalid_params(format!("Missing argument: {key}"), None))
}

fn opt_arg<'a>(args: Option<&'a JsonObject>, key: &str) -> Option<&'a str> {
    args.and_then(|a| a.get(key))
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
}

fn schema_clause(schema: Option<&str>) -> String {
    schema.map_or(String::new(), |s| format!(" (schema `{s}`)"))
}

fn audit_table(args: Option<&JsonObject>) -> Result<String, McpError> {
    let connection = arg(args, "connection_id")?;
    let database = arg(args, "database")?;
    let table = arg(args, "table")?;
    let schema = opt_arg(args, "schema");
    Ok(format!(
        "Audit the table `{table}` in database `{database}`{} on QoreDB connection `{connection}`.\n\
         \n\
         1. Call `describe_table` and summarise columns, types, nullability, primary key, foreign keys and indexes.\n\
         2. Call `preview_table` with a small limit to see representative values; never quote more than a few rows.\n\
         3. Estimate the volume from `row_count_estimate`, or with a `SELECT count(*)` through `run_query` if it is missing.\n\
         4. Flag columns that look sensitive (emails, phone numbers, names, addresses, tokens, secrets, card or account numbers).\n\
         5. Point out missing indexes on foreign keys, nullable columns that are never null in the sample, and unusual types.\n\
         \n\
         Every tool is read-only; do not attempt any modification. End with a short list of recommended actions.",
        schema_clause(schema)
    ))
}

fn explain_slow_query(args: Option<&JsonObject>) -> Result<String, McpError> {
    let connection = arg(args, "connection_id")?;
    let database = arg(args, "database")?;
    let query = arg(args, "query")?;
    Ok(format!(
        "Analyse this slow query on QoreDB connection `{connection}`, database `{database}`:\n\
         \n\
         ```sql\n{query}\n```\n\
         \n\
         1. Call `explain_query` with the query to obtain the execution plan.\n\
         2. Call `describe_table` on every table involved to know the available indexes and key columns.\n\
         3. Identify sequential scans, nested loops on large inputs, sorts spilling to disk and missing index usage.\n\
         4. Propose concrete improvements: rewritten query, indexes to add (with the exact DDL), or schema changes.\n\
         \n\
         Only reading tools are available; present the DDL as suggestions for the user to apply."
    ))
}

fn document_schema(args: Option<&JsonObject>) -> Result<String, McpError> {
    let connection = arg(args, "connection_id")?;
    let database = arg(args, "database")?;
    let schema = opt_arg(args, "schema");
    Ok(format!(
        "Document the database `{database}`{} of QoreDB connection `{connection}` in Markdown.\n\
         \n\
         1. Call `list_tables` to enumerate the tables, then `describe_table` on each one.\n\
         2. For every table write a heading, a one-sentence purpose inferred from its name and columns, \
         a column table (name, type, nullable, default, key), and the relations declared by foreign keys.\n\
         3. Finish with a relations overview listing each foreign key as `table.column -> referenced_table.column`.\n\
         \n\
         Do not preview or quote row data: the documentation must describe structure only.",
        schema_clause(schema)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(name: &str, args: serde_json::Value) -> GetPromptRequestParams {
        let params = GetPromptRequestParams::new(name);
        match args.as_object() {
            Some(map) => params.with_arguments(map.clone()),
            None => params,
        }
    }

    #[test]
    fn every_definition_renders_with_its_required_arguments() {
        for prompt in definitions() {
            let mut args = serde_json::Map::new();
            for a in prompt.arguments.iter().flatten() {
                args.insert(a.name.clone(), serde_json::Value::String("x".into()));
            }
            let result = render(&params(&prompt.name, serde_json::Value::Object(args))).unwrap();
            assert_eq!(result.messages.len(), 1);
        }
    }

    #[test]
    fn missing_required_argument_is_an_invalid_params_error() {
        let err = render(&params(
            "audit_table",
            serde_json::json!({ "connection_id": "c", "database": "d" }),
        ))
        .unwrap_err();
        assert!(err.message.contains("table"));
        assert!(render(&params("nope", serde_json::json!({}))).is_err());
    }
}
