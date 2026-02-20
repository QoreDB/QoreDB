// SPDX-License-Identifier: Apache-2.0

use base64::{engine::general_purpose::STANDARD, Engine as _};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::fs::File;

use crate::engine::types::{ColumnInfo, Row, Value};
use crate::export::writers::ExportWriter;

pub struct JsonWriter {
    writer: BufWriter<File>,
    bytes_written: u64,
    started: bool,
    rows_written: u64,
}

impl JsonWriter {
    pub fn new(writer: BufWriter<File>) -> Self {
        Self {
            writer,
            bytes_written: 0,
            started: false,
            rows_written: 0,
        }
    }

    async fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.writer
            .write_all(bytes)
            .await
            .map_err(|e| e.to_string())?;
        self.bytes_written += bytes.len() as u64;
        Ok(())
    }

    async fn ensure_started(&mut self) -> Result<(), String> {
        if !self.started {
            self.write_bytes(b"[\n").await?;
            self.started = true;
        }
        Ok(())
    }

    fn value_to_json(value: &Value) -> serde_json::Value {
        match value {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::Value::Bool(*b),
            Value::Int(i) => serde_json::Value::Number((*i).into()),
            Value::Float(f) => {
                serde_json::Number::from_f64(*f)
                    .map(serde_json::Value::Number)
                    .unwrap_or_else(|| serde_json::Value::String(f.to_string()))
            }
            Value::Text(s) => serde_json::Value::String(s.clone()),
            Value::Bytes(b) => serde_json::Value::String(STANDARD.encode(b)),
            Value::Json(j) => j.clone(),
            Value::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(Self::value_to_json).collect())
            }
        }
    }
}

#[async_trait::async_trait]
impl ExportWriter for JsonWriter {
    async fn write_header(&mut self, _columns: &[ColumnInfo]) -> Result<(), String> {
        self.ensure_started().await
    }

    async fn write_row(&mut self, columns: &[ColumnInfo], row: &Row) -> Result<(), String> {
        self.ensure_started().await?;

        let mut obj = serde_json::Map::with_capacity(columns.len());
        for (idx, col) in columns.iter().enumerate() {
            let value = row.values.get(idx).unwrap_or(&Value::Null);
            obj.insert(col.name.clone(), Self::value_to_json(value));
        }

        let json = serde_json::Value::Object(obj);
        let serialized = serde_json::to_string(&json).map_err(|e| e.to_string())?;

        if self.rows_written > 0 {
            self.write_bytes(b",\n").await?;
        }
        self.write_bytes(serialized.as_bytes()).await?;
        self.rows_written += 1;
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), String> {
        self.writer.flush().await.map_err(|e| e.to_string())
    }

    async fn finish(&mut self) -> Result<(), String> {
        if !self.started {
            self.write_bytes(b"[]\n").await?;
            return self.flush().await;
        }

        if self.rows_written > 0 {
            self.write_bytes(b"\n").await?;
        }
        self.write_bytes(b"]\n").await?;
        self.flush().await
    }

    fn bytes_written(&self) -> u64 {
        self.bytes_written
    }
}
