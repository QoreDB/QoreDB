// SPDX-License-Identifier: Apache-2.0

use tokio::fs::File;
use tokio::io::BufWriter;

use crate::engine::sql_generator::SqlDialect;
use crate::engine::types::{ColumnInfo, Namespace, Row, Value};
use crate::export::writers::counting::CountingWriter;
use crate::export::writers::ExportWriter;

pub struct SqlInsertWriter {
    writer: CountingWriter,
    dialect: SqlDialect,
    namespace: Option<Namespace>,
    table_name: String,
    columns_sql: Option<String>,
}

impl SqlInsertWriter {
    pub fn new(
        writer: BufWriter<File>,
        dialect: SqlDialect,
        namespace: Option<Namespace>,
        table_name: String,
    ) -> Self {
        Self {
            writer: CountingWriter::new(writer),
            dialect,
            namespace,
            table_name,
            columns_sql: None,
        }
    }

    fn qualified_table(&self) -> String {
        if let Some(ref ns) = self.namespace {
            self.dialect.qualified_table(ns, &self.table_name)
        } else {
            self.dialect.quote_ident(&self.table_name)
        }
    }

    fn ensure_columns(&mut self, columns: &[ColumnInfo]) {
        if self.columns_sql.is_none() {
            let cols = columns
                .iter()
                .map(|col| self.dialect.quote_ident(&col.name))
                .collect::<Vec<_>>()
                .join(", ");
            self.columns_sql = Some(cols);
        }
    }

    fn format_value(&self, value: &Value) -> String {
        self.dialect.format_value(value)
    }
}

#[async_trait::async_trait]
impl ExportWriter for SqlInsertWriter {
    async fn write_header(&mut self, columns: &[ColumnInfo]) -> Result<(), String> {
        self.ensure_columns(columns);
        Ok(())
    }

    async fn write_row(&mut self, columns: &[ColumnInfo], row: &Row) -> Result<(), String> {
        if columns.is_empty() {
            return Err("No columns available for SQL export".to_string());
        }

        self.ensure_columns(columns);
        let columns_sql = self.columns_sql.clone().unwrap_or_default();
        let mut values = Vec::with_capacity(columns.len());

        for idx in 0..columns.len() {
            let value = row.values.get(idx).unwrap_or(&Value::Null);
            values.push(self.format_value(value));
        }

        let insert = format!(
            "INSERT INTO {} ({}) VALUES ({});",
            self.qualified_table(),
            columns_sql,
            values.join(", ")
        );

        self.writer.write_line(&insert).await
    }

    async fn flush(&mut self) -> Result<(), String> {
        self.writer.flush().await
    }

    async fn finish(&mut self) -> Result<(), String> {
        self.flush().await
    }

    fn bytes_written(&self) -> u64 {
        self.writer.bytes_written()
    }
}
