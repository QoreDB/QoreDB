// SPDX-License-Identifier: Apache-2.0

use base64::{engine::general_purpose::STANDARD, Engine as _};
use tokio::fs::File;
use tokio::io::BufWriter;

use crate::engine::types::{ColumnInfo, Row, Value};
use crate::export::writers::counting::CountingWriter;
use crate::export::writers::ExportWriter;

pub struct CsvWriter {
    writer: CountingWriter,
    include_headers: bool,
    header_written: bool,
}

impl CsvWriter {
    pub fn new(writer: BufWriter<File>, include_headers: bool) -> Self {
        Self {
            writer: CountingWriter::new(writer),
            include_headers,
            header_written: false,
        }
    }

    fn escape_csv(value: &str) -> String {
        if value.contains(',')
            || value.contains('"')
            || value.contains('\n')
            || value.contains('\r')
        {
            format!("\"{}\"", value.replace('"', "\"\""))
        } else {
            value.to_string()
        }
    }

    fn format_value(value: &Value) -> String {
        match value {
            Value::Null => "".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Text(s) => s.clone(),
            Value::Bytes(b) => STANDARD.encode(b),
            Value::Json(j) => j.to_string(),
            Value::Array(arr) => serde_json::to_string(arr).unwrap_or_else(|_| "[]".to_string()),
        }
    }
}

#[async_trait::async_trait]
impl ExportWriter for CsvWriter {
    async fn write_header(&mut self, columns: &[ColumnInfo]) -> Result<(), String> {
        if !self.include_headers || self.header_written || columns.is_empty() {
            return Ok(());
        }

        let header = columns
            .iter()
            .map(|col| Self::escape_csv(&col.name))
            .collect::<Vec<_>>()
            .join(",");

        self.writer.write_line(&header).await?;
        self.header_written = true;
        Ok(())
    }

    async fn write_row(&mut self, columns: &[ColumnInfo], row: &Row) -> Result<(), String> {
        if columns.is_empty() {
            return Ok(());
        }

        let mut fields = Vec::with_capacity(columns.len());
        for idx in 0..columns.len() {
            let value = row.values.get(idx).unwrap_or(&Value::Null);
            let formatted = Self::format_value(value);
            fields.push(Self::escape_csv(&formatted));
        }

        let line = fields.join(",");
        self.writer.write_line(&line).await
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
