// SPDX-License-Identifier: BUSL-1.1

use std::sync::Arc;

use arrow::array::{
    ArrayRef, BinaryArray, BooleanArray, Float64Array, Int64Array, NullArray, StringArray,
};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

use crate::engine::types::{ColumnInfo, Row, Value};
use crate::export::writers::ExportWriter;

/// Batch size for writing row groups to Parquet.
const ROW_GROUP_SIZE: usize = 10_000;

pub struct ParquetExportWriter {
    output_path: String,
    schema: Option<Arc<Schema>>,
    columns: Vec<ColumnInfo>,
    buffered_rows: Vec<Vec<Value>>,
    bytes_written: u64,
    writer: Option<ArrowWriter<std::fs::File>>,
}

impl ParquetExportWriter {
    pub fn new(output_path: String) -> Self {
        Self {
            output_path,
            schema: None,
            columns: Vec::new(),
            buffered_rows: Vec::new(),
            bytes_written: 0,
            writer: None,
        }
    }

    fn map_data_type(db_type: &str) -> DataType {
        let lower = db_type.to_lowercase();
        if lower.contains("bool") {
            DataType::Boolean
        } else if lower.contains("int") || lower.contains("serial") {
            DataType::Int64
        } else if lower.contains("float")
            || lower.contains("double")
            || lower.contains("real")
            || lower.contains("numeric")
            || lower.contains("decimal")
        {
            DataType::Float64
        } else if lower.contains("byte") || lower.contains("blob") || lower.contains("binary") {
            DataType::Binary
        } else {
            DataType::Utf8
        }
    }

    fn build_schema(columns: &[ColumnInfo]) -> Arc<Schema> {
        let fields: Vec<Field> = columns
            .iter()
            .map(|col| Field::new(col.name.as_str(), Self::map_data_type(&col.data_type), true))
            .collect();
        Arc::new(Schema::new(fields))
    }

    fn flush_buffer(&mut self) -> Result<(), String> {
        if self.buffered_rows.is_empty() {
            return Ok(());
        }

        let schema = self
            .schema
            .as_ref()
            .ok_or_else(|| "Schema not initialized".to_string())?;

        let num_cols = schema.fields().len();
        let mut arrays: Vec<ArrayRef> = Vec::with_capacity(num_cols);

        for col_idx in 0..num_cols {
            let arrow_type = schema.field(col_idx).data_type();
            let array = self.build_column_array(col_idx, arrow_type)?;
            arrays.push(array);
        }

        let batch = RecordBatch::try_new(Arc::clone(schema), arrays)
            .map_err(|e| format!("Failed to create record batch: {}", e))?;

        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| "Parquet writer not initialized".to_string())?;

        writer
            .write(&batch)
            .map_err(|e| format!("Failed to write row group: {}", e))?;

        self.buffered_rows.clear();
        Ok(())
    }

    fn build_column_array(
        &self,
        col_idx: usize,
        arrow_type: &DataType,
    ) -> Result<ArrayRef, String> {
        match arrow_type {
            DataType::Boolean => {
                let values: Vec<Option<bool>> = self
                    .buffered_rows
                    .iter()
                    .map(|row| match row.get(col_idx).unwrap_or(&Value::Null) {
                        Value::Bool(b) => Some(*b),
                        Value::Null => None,
                        v => Some(Self::value_to_string(v) == "true"),
                    })
                    .collect();
                Ok(Arc::new(BooleanArray::from(values)))
            }
            DataType::Int64 => {
                let values: Vec<Option<i64>> = self
                    .buffered_rows
                    .iter()
                    .map(|row| match row.get(col_idx).unwrap_or(&Value::Null) {
                        Value::Int(i) => Some(*i),
                        Value::Float(f) => Some(*f as i64),
                        Value::Bool(b) => Some(if *b { 1 } else { 0 }),
                        Value::Null => None,
                        v => Self::value_to_string(v).parse().ok(),
                    })
                    .collect();
                Ok(Arc::new(Int64Array::from(values)))
            }
            DataType::Float64 => {
                let values: Vec<Option<f64>> = self
                    .buffered_rows
                    .iter()
                    .map(|row| match row.get(col_idx).unwrap_or(&Value::Null) {
                        Value::Float(f) => Some(*f),
                        Value::Int(i) => Some(*i as f64),
                        Value::Null => None,
                        v => Self::value_to_string(v).parse().ok(),
                    })
                    .collect();
                Ok(Arc::new(Float64Array::from(values)))
            }
            DataType::Binary => {
                let values: Vec<Option<&[u8]>> = self
                    .buffered_rows
                    .iter()
                    .map(|row| match row.get(col_idx).unwrap_or(&Value::Null) {
                        Value::Bytes(b) => Some(b.as_slice()),
                        Value::Null => None,
                        _ => None,
                    })
                    .collect();
                Ok(Arc::new(BinaryArray::from(values)))
            }
            DataType::Utf8 => {
                let values: Vec<Option<String>> = self
                    .buffered_rows
                    .iter()
                    .map(|row| match row.get(col_idx).unwrap_or(&Value::Null) {
                        Value::Null => None,
                        v => Some(Self::value_to_string(v)),
                    })
                    .collect();
                let refs: Vec<Option<&str>> = values.iter().map(|v| v.as_deref()).collect();
                Ok(Arc::new(StringArray::from(refs)))
            }
            DataType::Null => Ok(Arc::new(NullArray::new(self.buffered_rows.len()))),
            _ => {
                let values: Vec<Option<String>> = self
                    .buffered_rows
                    .iter()
                    .map(|row| match row.get(col_idx).unwrap_or(&Value::Null) {
                        Value::Null => None,
                        v => Some(Self::value_to_string(v)),
                    })
                    .collect();
                let refs: Vec<Option<&str>> = values.iter().map(|v| v.as_deref()).collect();
                Ok(Arc::new(StringArray::from(refs)))
            }
        }
    }

    fn value_to_string(value: &Value) -> String {
        match value {
            Value::Null => String::new(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Text(s) => s.clone(),
            Value::Bytes(b) => {
                use base64::{Engine as _, engine::general_purpose::STANDARD};
                STANDARD.encode(b)
            }
            Value::Json(j) => j.to_string(),
            Value::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(Value::to_json).collect()).to_string()
            }
        }
    }
}

#[async_trait::async_trait]
impl ExportWriter for ParquetExportWriter {
    async fn write_header(&mut self, columns: &[ColumnInfo]) -> Result<(), String> {
        if columns.is_empty() {
            return Ok(());
        }

        self.columns = columns.to_vec();
        let schema = Self::build_schema(columns);
        self.schema = Some(Arc::clone(&schema));

        let file = std::fs::File::create(&self.output_path)
            .map_err(|e| format!("Failed to create Parquet file: {}", e))?;

        let props = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .build();

        let writer = ArrowWriter::try_new(file, schema, Some(props))
            .map_err(|e| format!("Failed to initialize Parquet writer: {}", e))?;

        self.writer = Some(writer);
        Ok(())
    }

    async fn write_row(&mut self, columns: &[ColumnInfo], row: &Row) -> Result<(), String> {
        if columns.is_empty() {
            return Ok(());
        }

        let values: Vec<Value> = (0..columns.len())
            .map(|i| row.values.get(i).cloned().unwrap_or(Value::Null))
            .collect();
        self.buffered_rows.push(values);

        if self.buffered_rows.len() >= ROW_GROUP_SIZE {
            self.flush_buffer()?;
        }

        Ok(())
    }

    async fn flush(&mut self) -> Result<(), String> {
        self.flush_buffer()
    }

    async fn finish(&mut self) -> Result<(), String> {
        self.flush_buffer()?;

        if let Some(writer) = self.writer.take() {
            let file_meta = writer
                .close()
                .map_err(|e| format!("Failed to finalize Parquet file: {}", e))?;

            for rg in file_meta.row_groups() {
                self.bytes_written += rg.total_byte_size() as u64;
            }
        }

        Ok(())
    }

    fn bytes_written(&self) -> u64 {
        self.bytes_written
    }
}

#[cfg(test)]
mod tests {
    use arrow::array::Array;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    use super::*;

    fn column(name: &str, data_type: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: data_type.into(),
            nullable: true,
        }
    }

    #[tokio::test]
    async fn writes_a_snappy_compressed_round_trip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("export.parquet");
        let columns = vec![
            column("enabled", "BOOLEAN"),
            column("count", "BIGINT"),
            column("ratio", "DOUBLE"),
            column("payload", "BLOB"),
            column("label", "TEXT"),
        ];
        let rows = [
            Row {
                values: vec![
                    Value::Bool(true),
                    Value::Int(42),
                    Value::Float(3.5),
                    Value::Bytes(vec![1, 2, 3]),
                    Value::Text("hello".to_string()),
                ],
            },
            Row {
                values: vec![Value::Null; columns.len()],
            },
        ];

        let mut writer = ParquetExportWriter::new(output_path.to_string_lossy().into_owned());
        writer.write_header(&columns).await.unwrap();
        for row in &rows {
            writer.write_row(&columns, row).await.unwrap();
        }
        writer.finish().await.unwrap();

        assert!(writer.bytes_written() > 0);

        let builder =
            ParquetRecordBatchReaderBuilder::try_new(std::fs::File::open(output_path).unwrap())
                .unwrap();
        assert_eq!(
            builder.metadata().row_group(0).column(0).compression(),
            Compression::SNAPPY
        );

        let mut reader = builder.build().unwrap();
        let batch = reader.next().unwrap().unwrap();
        assert_eq!(batch.num_rows(), 2);

        let enabled = batch
            .column(0)
            .as_any()
            .downcast_ref::<BooleanArray>()
            .unwrap();
        let count = batch
            .column(1)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        let ratio = batch
            .column(2)
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap();
        let payload = batch
            .column(3)
            .as_any()
            .downcast_ref::<BinaryArray>()
            .unwrap();
        let label = batch
            .column(4)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();

        assert!(enabled.value(0));
        assert_eq!(count.value(0), 42);
        assert_eq!(ratio.value(0), 3.5);
        assert_eq!(payload.value(0), &[1, 2, 3]);
        assert_eq!(label.value(0), "hello");
        assert!(enabled.is_null(1));
        assert!(count.is_null(1));
        assert!(ratio.is_null(1));
        assert!(payload.is_null(1));
        assert!(label.is_null(1));
    }
}
