// SPDX-License-Identifier: Apache-2.0

use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};

pub struct CountingWriter {
    writer: BufWriter<File>,
    bytes_written: u64,
}

impl CountingWriter {
    pub fn new(writer: BufWriter<File>) -> Self {
        Self {
            writer,
            bytes_written: 0,
        }
    }

    pub async fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.writer
            .write_all(bytes)
            .await
            .map_err(|e| e.to_string())?;
        self.bytes_written += bytes.len() as u64;
        Ok(())
    }

    pub async fn write_line(&mut self, line: &str) -> Result<(), String> {
        self.write_bytes(line.as_bytes()).await?;
        self.write_bytes(b"\n").await?;
        Ok(())
    }

    pub async fn flush(&mut self) -> Result<(), String> {
        self.writer.flush().await.map_err(|e| e.to_string())
    }

    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }
}
