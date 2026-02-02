use async_trait::async_trait;
use tokio::fs::File;
use tokio::io::BufWriter;

use crate::engine::types::{ColumnInfo, Namespace, Row};
use crate::engine::sql_generator::SqlDialect;
use crate::export::types::ExportFormat;

pub mod csv;
pub mod json;
pub mod sql;

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
        ExportFormat::Csv => Ok(Box::new(csv::CsvWriter::new(writer, include_headers))),
        ExportFormat::Json => Ok(Box::new(json::JsonWriter::new(writer))),
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
            )))
        }
    }
}
