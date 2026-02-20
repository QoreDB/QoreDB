// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use tokio::fs::File;
use tokio::io::BufWriter;

use crate::engine::types::{ColumnInfo, Namespace, Row};
use crate::engine::sql_generator::SqlDialect;
use crate::export::types::ExportFormat;

pub mod csv;
pub mod html;
pub mod json;
pub mod sql;

#[cfg(feature = "pro")]
pub mod parquet_writer;
#[cfg(feature = "pro")]
pub mod xlsx;

#[async_trait]
pub trait ExportWriter: Send {
    async fn write_header(&mut self, columns: &[ColumnInfo]) -> Result<(), String>;
    async fn write_row(&mut self, columns: &[ColumnInfo], row: &Row) -> Result<(), String>;
    async fn flush(&mut self) -> Result<(), String>;
    async fn finish(&mut self) -> Result<(), String>;
    fn bytes_written(&self) -> u64;
}

pub async fn create_writer(
    format: ExportFormat,
    output_path: &str,
    include_headers: bool,
    table_name: Option<String>,
    namespace: Option<Namespace>,
    driver_id: &str,
) -> Result<Box<dyn ExportWriter>, String> {
    let file = File::create(output_path)
        .await
        .map_err(|e| format!("Failed to create export file: {}", e))?;
    let writer = BufWriter::new(file);

    match format {
        ExportFormat::Csv => Ok(
            Box::new(csv::CsvWriter::new(writer, include_headers)) as Box<dyn ExportWriter>
        ),
        ExportFormat::Json => Ok(Box::new(json::JsonWriter::new(writer)) as Box<dyn ExportWriter>),
        ExportFormat::Html => Ok(Box::new(html::HtmlWriter::new(writer)) as Box<dyn ExportWriter>),
        ExportFormat::SqlInsert => {
            let table = table_name
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| "Table name is required for SQL INSERT export".to_string())?;
            let dialect = SqlDialect::from_driver_id(driver_id)
                .ok_or_else(|| "SQL INSERT export is not supported for this driver".to_string())?;
            Ok(Box::new(sql::SqlInsertWriter::new(
                writer,
                dialect,
                namespace,
                table,
            )) as Box<dyn ExportWriter>)
        }
        #[cfg(feature = "pro")]
        ExportFormat::Xlsx => {
            // XLSX writer manages its own file I/O
            drop(writer);
            Ok(Box::new(xlsx::XlsxWriter::new(output_path.to_string())) as Box<dyn ExportWriter>)
        }
        #[cfg(feature = "pro")]
        ExportFormat::Parquet => {
            // Parquet writer manages its own file I/O
            drop(writer);
            Ok(Box::new(parquet_writer::ParquetExportWriter::new(
                output_path.to_string(),
            )) as Box<dyn ExportWriter>)
        }
        #[cfg(not(feature = "pro"))]
        ExportFormat::Xlsx | ExportFormat::Parquet => {
            Err("XLSX and Parquet export require QoreDB Pro".to_string())
        }
    }
}
