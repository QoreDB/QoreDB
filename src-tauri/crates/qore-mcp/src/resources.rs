// SPDX-License-Identifier: Apache-2.0

//! `qore://{connection_id}` lists a connection's namespaces and tables;
//! `qore://{connection_id}/{database}[/{schema}]/{table}` resolves to a table's
//! `describe_table`. Only connections are enumerated by `resources/list`, so
//! listing costs no network round trip; tables come through the template.

use qore_core::Namespace;
use rmcp::model::{AnnotateAble, RawResource, RawResourceTemplate, Resource, ResourceTemplate};

pub const SCHEME: &str = "qore://";
pub const MIME_TYPE: &str = "application/json";
/// Caps the tables listed per namespace in a connection overview.
pub const MAX_TABLES_PER_NAMESPACE: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRef {
    pub connection_id: String,
    pub namespace: Namespace,
    pub table: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceRef {
    Connection(String),
    Table(TableRef),
}

pub fn format_uri(connection_id: &str, namespace: &Namespace, table: &str) -> String {
    match namespace.schema.as_deref() {
        Some(schema) => format!(
            "{SCHEME}{connection_id}/{}/{schema}/{table}",
            namespace.database
        ),
        None => format!("{SCHEME}{connection_id}/{}/{table}", namespace.database),
    }
}

pub fn parse_uri(uri: &str) -> Result<ResourceRef, String> {
    let rest = uri
        .strip_prefix(SCHEME)
        .ok_or_else(|| format!("Unsupported resource URI: {uri}"))?;
    let parts: Vec<&str> = rest.split('/').collect();
    let valid = matches!(parts.len(), 1 | 3 | 4) && parts.iter().all(|p| !p.is_empty());
    if !valid {
        return Err(format!(
            "Invalid resource URI '{uri}': expected {SCHEME}{{connection_id}} or              {SCHEME}{{connection_id}}/{{database}}[/{{schema}}]/{{table}}"
        ));
    }
    if parts.len() == 1 {
        return Ok(ResourceRef::Connection(parts[0].to_string()));
    }
    let schema = (parts.len() == 4).then(|| parts[2].to_string());
    Ok(ResourceRef::Table(TableRef {
        connection_id: parts[0].to_string(),
        namespace: Namespace {
            database: parts[1].to_string(),
            schema,
        },
        table: parts[parts.len() - 1].to_string(),
    }))
}

pub fn connection_resource(connection_id: &str, name: &str, driver: &str) -> Resource {
    let mut raw = RawResource::new(format!("{SCHEME}{connection_id}"), name)
        .with_title(format!("{name} ({driver})"));
    raw.description = Some(
        "Namespaces and tables of this connection; read a table with          qore://{connection_id}/{database}[/{schema}]/{table}"
            .to_string(),
    );
    raw.mime_type = Some(MIME_TYPE.to_string());
    raw.no_annotation()
}

pub fn template() -> ResourceTemplate {
    let mut raw = RawResourceTemplate::new(
        format!("{SCHEME}{{connection_id}}/{{database}}/{{table}}"),
        "table-schema",
    );
    raw.title = Some("Table schema".to_string());
    raw.description = Some(
        "Schema of one table on an exposed connection. Add a schema segment before the \
         table for engines that use schemas: qore://{connection_id}/{database}/{schema}/{table}"
            .to_string(),
    );
    raw.mime_type = Some(MIME_TYPE.to_string());
    raw.no_annotation()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_roundtrips_with_and_without_schema() {
        let plain = Namespace::new("shop");
        let uri = format_uri("conn_1", &plain, "orders");
        assert_eq!(uri, "qore://conn_1/shop/orders");
        assert_eq!(
            parse_uri(&uri).unwrap(),
            ResourceRef::Table(TableRef {
                connection_id: "conn_1".into(),
                namespace: plain,
                table: "orders".into(),
            })
        );

        let scoped = Namespace::with_schema("shop", "public");
        let uri = format_uri("conn_1", &scoped, "orders");
        assert_eq!(uri, "qore://conn_1/shop/public/orders");
        match parse_uri(&uri).unwrap() {
            ResourceRef::Table(table) => assert_eq!(table.namespace, scoped),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(
            parse_uri("qore://conn_1").unwrap(),
            ResourceRef::Connection("conn_1".into())
        );
    }

    #[test]
    fn malformed_uris_are_rejected() {
        assert!(parse_uri("file:///etc/passwd").is_err());
        assert!(parse_uri("qore://conn_1/shop").is_err());
        assert!(parse_uri("qore://conn_1//orders").is_err());
        assert!(parse_uri("qore://a/b/c/d/e").is_err());
    }
}
