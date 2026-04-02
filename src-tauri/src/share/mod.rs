// SPDX-License-Identifier: Apache-2.0

mod manager;
pub mod types;

use crate::engine::types::QueryResult;
use crate::export::types::ExportFormat;
use crate::export::writers::create_writer;

pub use manager::ShareManager;

pub async fn write_query_result_to_file(
    format: ExportFormat,
    output_path: &str,
    include_headers: bool,
    table_name: Option<String>,
    namespace: Option<crate::engine::types::Namespace>,
    driver_id: &str,
    result: &QueryResult,
    limit: Option<u64>,
) -> Result<u64, String> {
    let mut writer = create_writer(
        format,
        output_path,
        include_headers,
        table_name,
        namespace,
        driver_id,
    )
    .await?;

    writer.write_header(&result.columns).await?;

    let mut rows_written = 0u64;
    for row in &result.rows {
        if let Some(limit) = limit {
            if rows_written >= limit {
                break;
            }
        }
        writer.write_row(&result.columns, row).await?;
        rows_written += 1;
    }

    writer.flush().await?;
    writer.finish().await?;

    Ok(rows_written)
}
