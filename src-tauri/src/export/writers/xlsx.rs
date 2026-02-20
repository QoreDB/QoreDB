// SPDX-License-Identifier: BUSL-1.1

use rust_xlsxwriter::{Format, Workbook};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::engine::types::{ColumnInfo, Row, Value};
use crate::export::writers::ExportWriter;

pub struct XlsxWriter {
    workbook: Workbook,
    current_row: u32,
    bytes_written: u64,
    output_path: String,
    header_format: Format,
}

impl XlsxWriter {
    pub fn new(output_path: String) -> Self {
        let header_format = Format::new().set_bold();

        Self {
            workbook: Workbook::new(),
            current_row: 0,
            bytes_written: 0,
            output_path,
            header_format,
        }
    }

    fn write_value(
        worksheet: &mut rust_xlsxwriter::Worksheet,
        row: u32,
        col: u16,
        value: &Value,
    ) -> Result<(), String> {
        match value {
            Value::Null => {
                worksheet.write_string(row, col, "").map_err(|e| e.to_string())?;
            }
            Value::Bool(b) => {
                worksheet
                    .write_boolean(row, col, *b)
                    .map_err(|e| e.to_string())?;
            }
            Value::Int(i) => {
                worksheet
                    .write_number(row, col, *i as f64)
                    .map_err(|e| e.to_string())?;
            }
            Value::Float(f) => {
                worksheet
                    .write_number(row, col, *f)
                    .map_err(|e| e.to_string())?;
            }
            Value::Text(s) => {
                worksheet
                    .write_string(row, col, s)
                    .map_err(|e| e.to_string())?;
            }
            Value::Bytes(b) => {
                use base64::{engine::general_purpose::STANDARD, Engine as _};
                worksheet
                    .write_string(row, col, &STANDARD.encode(b))
                    .map_err(|e| e.to_string())?;
            }
            Value::Json(j) => {
                worksheet
                    .write_string(row, col, &j.to_string())
                    .map_err(|e| e.to_string())?;
            }
            Value::Array(arr) => {
                let s = serde_json::to_string(arr).unwrap_or_else(|_| "[]".to_string());
                worksheet
                    .write_string(row, col, &s)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl ExportWriter for XlsxWriter {
    async fn write_header(&mut self, columns: &[ColumnInfo]) -> Result<(), String> {
        if columns.is_empty() {
            return Ok(());
        }

        let worksheet = self
            .workbook
            .add_worksheet()
            .set_name("Export")
            .map_err(|e| e.to_string())?;

        for (col_idx, col) in columns.iter().enumerate() {
            worksheet
                .write_string_with_format(0, col_idx as u16, &col.name, &self.header_format)
                .map_err(|e| e.to_string())?;
        }

        self.current_row = 1;
        Ok(())
    }

    async fn write_row(&mut self, columns: &[ColumnInfo], row: &Row) -> Result<(), String> {
        if columns.is_empty() {
            return Ok(());
        }

        let worksheet = self
            .workbook
            .worksheet_from_index(0)
            .map_err(|e| e.to_string())?;

        for idx in 0..columns.len() {
            let value = row.values.get(idx).unwrap_or(&Value::Null);
            Self::write_value(worksheet, self.current_row, idx as u16, value)?;
        }

        self.current_row += 1;
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), String> {
        // XLSX writes are in-memory until finish()
        Ok(())
    }

    async fn finish(&mut self) -> Result<(), String> {
        let buffer = self
            .workbook
            .save_to_buffer()
            .map_err(|e| format!("Failed to generate XLSX: {}", e))?;

        self.bytes_written = buffer.len() as u64;

        let mut file = File::create(&self.output_path)
            .await
            .map_err(|e| format!("Failed to create output file: {}", e))?;

        file.write_all(&buffer)
            .await
            .map_err(|e| format!("Failed to write XLSX: {}", e))?;

        file.flush()
            .await
            .map_err(|e| format!("Failed to flush XLSX: {}", e))?;

        Ok(())
    }

    fn bytes_written(&self) -> u64 {
        self.bytes_written
    }
}
