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
                worksheet
                    .write_string(row, col, "")
                    .map_err(|e| e.to_string())?;
            }
            Value::Bool(b) => {
                worksheet
                    .write_boolean(row, col, *b)
                    .map_err(|e| e.to_string())?;
            }
            // Excel only guarantees 15 decimal digits for numeric cells.
            // Store larger identifiers as text rather than changing their value.
            Value::Int(i) if !(-999_999_999_999_999..=999_999_999_999_999).contains(i) => {
                worksheet
                    .write_string(row, col, i.to_string())
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
                use base64::{Engine as _, engine::general_purpose::STANDARD};
                worksheet
                    .write_string(row, col, STANDARD.encode(b))
                    .map_err(|e| e.to_string())?;
            }
            Value::Json(j) => {
                worksheet
                    .write_string(row, col, j.to_string())
                    .map_err(|e| e.to_string())?;
            }
            Value::Array(arr) => {
                let s =
                    serde_json::Value::Array(arr.iter().map(Value::to_json).collect()).to_string();
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
                .write_string_with_format(0, col_idx as u16, col.name.as_str(), &self.header_format)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn xml(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> String {
        let mut content = String::new();
        archive
            .by_name(name)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        content
    }

    #[tokio::test]
    async fn large_integers_remain_exact_text_in_the_generated_workbook() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exact.xlsx");
        let columns = vec![ColumnInfo {
            name: "id".into(),
            data_type: "BIGINT".into(),
            nullable: true,
            masked: false,
        }];
        let mut writer = XlsxWriter::new(path.to_string_lossy().into_owned());
        writer.write_header(&columns).await.unwrap();
        for value in [
            999_999_999_999_999,
            1_000_000_000_000_001,
            9_007_199_254_740_993,
            i64::MAX,
            i64::MIN,
        ] {
            writer
                .write_row(
                    &columns,
                    &Row {
                        values: vec![Value::Int(value)],
                    },
                )
                .await
                .unwrap();
        }
        writer.finish().await.unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let sheet = xml(&mut archive, "xl/worksheets/sheet1.xml");
        let strings = xml(&mut archive, "xl/sharedStrings.xml");
        assert!(sheet.contains("<v>999999999999999</v>"));
        for (row, value) in [
            (3, "1000000000000001"),
            (4, "9007199254740993"),
            (5, "9223372036854775807"),
            (6, "-9223372036854775808"),
        ] {
            assert!(
                sheet.contains(&format!("<c r=\"A{row}\" t=\"s\">")),
                "large integer is stored as text at row {row}"
            );
            assert!(strings.contains(&format!("<t>{value}</t>")));
        }
        assert_eq!(
            writer.bytes_written(),
            std::fs::metadata(path).unwrap().len()
        );
    }

    #[tokio::test]
    async fn text_decimals_and_formula_like_values_are_preserved_without_formulas() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("text.xlsx");
        let columns = vec![ColumnInfo {
            name: "value".into(),
            data_type: "TEXT".into(),
            nullable: true,
            masked: false,
        }];
        let mut writer = XlsxWriter::new(path.to_string_lossy().into_owned());
        writer.write_header(&columns).await.unwrap();
        for value in [
            Value::Text("0.123456789012345678901234567890".into()),
            Value::Text("=1+1".into()),
            Value::Text("••••••".into()),
            Value::Null,
            Value::Bool(true),
        ] {
            writer
                .write_row(
                    &columns,
                    &Row {
                        values: vec![value],
                    },
                )
                .await
                .unwrap();
        }
        writer.finish().await.unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        let sheet = xml(&mut archive, "xl/worksheets/sheet1.xml");
        let strings = xml(&mut archive, "xl/sharedStrings.xml");
        assert!(strings.contains("0.123456789012345678901234567890"));
        assert!(strings.contains("=1+1"));
        assert!(strings.contains("••••••"));
        assert!(!sheet.contains("<f>"));
        assert!(sheet.contains("t=\"b\"><v>1</v>"));
    }
}
