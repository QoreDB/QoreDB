// SPDX-License-Identifier: BUSL-1.1

use std::io::Read;
use std::path::Path;

use arrow::array::{Array, Int64Array, StringArray};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use qore_core::masking::{ConnectionMasking, MaskMode, MaskingRule, SessionMasking};

use super::*;
use crate::engine::drivers::sqlite::SqliteDriver;
use crate::engine::types::ConnectionConfig;

const EXACT: &str = "123456789012345678.12345678901234567890";
const PRIVATE: &str = "synthetic-private@example.invalid";

async fn database(path: &Path) -> (Arc<SqliteDriver>, SessionId) {
    let driver = Arc::new(SqliteDriver::new());
    let config = ConnectionConfig {
        driver: "sqlite".into(),
        host: path.to_string_lossy().into_owned(),
        port: 0,
        username: String::new(),
        password: String::new(),
        database: None,
        ssl: false,
        ssl_mode: None,
        environment: "development".into(),
        read_only: false,
        ssh_tunnel: None,
        pool_acquire_timeout_secs: None,
        pool_max_connections: None,
        pool_min_connections: None,
        proxy: None,
        mssql_auth: None,
        clickhouse_cluster: None,
        search_auth_mode: None,
        ssl_ca_cert: None,
        options: Default::default(),
    };
    let session = driver.connect(&config).await.unwrap();
    for query in [
        "CREATE TABLE export_fixture (id BIGINT, amount TEXT, email TEXT, missing TEXT)",
        "INSERT INTO export_fixture VALUES (9007199254740993, '123456789012345678.12345678901234567890', 'synthetic-private@example.invalid', NULL)",
    ] {
        driver
            .execute(session, query, QueryId::new())
            .await
            .unwrap();
    }
    (driver, session)
}

fn read_xlsx(path: &Path, name: &str) -> String {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut xml = String::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    xml
}

async fn export(
    driver: Arc<SqliteDriver>,
    session: SessionId,
    path: &Path,
    format: ExportFormat,
    query: &str,
    masked: bool,
) -> ExportProgress {
    let masking = masked.then(|| {
        Arc::new(SessionMasking {
            connection_id: "synthetic-connection".into(),
            config: ConnectionMasking {
                rules: vec![MaskingRule {
                    table: String::new(),
                    column: "email".into(),
                    mode: MaskMode::Hidden,
                }],
                mask_detected_columns: false,
            },
        })
    });
    let config = ExportConfig {
        query: query.into(),
        namespace: None,
        output_path: path.to_string_lossy().into_owned(),
        format,
        table_name: Some("export_fixture".into()),
        include_headers: true,
        batch_size: Some(1000),
        limit: None,
    };
    let mut progress = Vec::new();
    run_export_task(
        driver,
        "sqlite".into(),
        masking,
        session,
        config,
        "sqlite-export".into(),
        CancellationToken::new(),
        |event| progress.push(event),
    )
    .await
    .unwrap();
    progress.pop().unwrap()
}

#[tokio::test]
async fn sqlite_exports_round_trip_all_formats_with_and_without_masking() {
    let dir = tempfile::tempdir().unwrap();
    let (driver, session) = database(&dir.path().join("source.db")).await;
    for masked in [false, true] {
        for format in [
            ExportFormat::Csv,
            ExportFormat::Json,
            ExportFormat::Html,
            ExportFormat::SqlInsert,
            ExportFormat::Xlsx,
            ExportFormat::Parquet,
        ] {
            let path = dir.path().join("result.export");
            let progress = export(
                driver.clone(),
                session,
                &path,
                format.clone(),
                "SELECT id, amount, email, missing FROM export_fixture",
                masked,
            )
            .await;
            assert_eq!(
                progress.state,
                ExportState::Completed,
                "{format:?}: {:?}",
                progress.error
            );
            assert_eq!(progress.rows_exported, 1);
            assert_eq!(
                progress.bytes_written,
                std::fs::metadata(&path).unwrap().len()
            );
            let expected_email = if masked {
                qore_core::masking::HIDDEN_VALUE
            } else {
                PRIVATE
            };
            match format {
                ExportFormat::Parquet => {
                    let mut reader = ParquetRecordBatchReaderBuilder::try_new(
                        std::fs::File::open(&path).unwrap(),
                    )
                    .unwrap()
                    .build()
                    .unwrap();
                    let batch = reader.next().unwrap().unwrap();
                    assert_eq!(batch.num_rows(), 1);
                    assert_eq!(
                        batch
                            .column(0)
                            .as_any()
                            .downcast_ref::<Int64Array>()
                            .unwrap()
                            .value(0),
                        9007199254740993
                    );
                    assert_eq!(
                        batch
                            .column(1)
                            .as_any()
                            .downcast_ref::<StringArray>()
                            .unwrap()
                            .value(0),
                        EXACT
                    );
                    assert_eq!(
                        batch
                            .column(2)
                            .as_any()
                            .downcast_ref::<StringArray>()
                            .unwrap()
                            .value(0),
                        expected_email
                    );
                    assert!(batch.column(3).is_null(0));
                    assert!(reader.next().is_none());
                }
                ExportFormat::Xlsx => {
                    let strings = read_xlsx(&path, "xl/sharedStrings.xml");
                    assert!(strings.contains("9007199254740993"));
                    assert!(strings.contains(EXACT));
                    assert!(strings.contains(expected_email));
                    if masked {
                        assert!(!strings.contains(PRIVATE));
                    }
                    assert!(!read_xlsx(&path, "xl/worksheets/sheet1.xml").contains("<f>"));
                }
                _ => {
                    let text = std::fs::read_to_string(&path).unwrap();
                    assert!(text.contains("9007199254740993"));
                    assert!(text.contains(EXACT));
                    assert!(text.contains(expected_email), "{format:?}: {text}");
                    if masked {
                        assert!(!text.contains(PRIVATE));
                    }
                    if matches!(format, ExportFormat::Json) {
                        let rows: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
                        assert_eq!(rows.len(), 1);
                        assert!(rows[0]["missing"].is_null());
                    }
                }
            }
        }
    }
    driver.disconnect(session).await.unwrap();
}

#[tokio::test]
async fn sqlite_empty_exports_are_valid_and_keep_column_headers() {
    let dir = tempfile::tempdir().unwrap();
    let (driver, session) = database(&dir.path().join("source.db")).await;
    for format in [ExportFormat::Parquet, ExportFormat::Xlsx, ExportFormat::Csv] {
        let path = dir.path().join("empty.export");
        let progress = export(
            driver.clone(),
            session,
            &path,
            format.clone(),
            "SELECT id, amount, email, missing FROM export_fixture WHERE 0",
            false,
        )
        .await;
        assert_eq!(
            progress.state,
            ExportState::Completed,
            "{:?}",
            progress.error
        );
        assert_eq!(progress.rows_exported, 0);
        match format {
            ExportFormat::Parquet => {
                let builder =
                    ParquetRecordBatchReaderBuilder::try_new(std::fs::File::open(&path).unwrap())
                        .unwrap();
                assert_eq!(builder.schema().fields().len(), 4);
                assert_eq!(builder.schema().field(0).name(), "id");
                assert!(builder.build().unwrap().next().is_none());
            }
            ExportFormat::Xlsx => {
                assert!(read_xlsx(&path, "xl/sharedStrings.xml").contains("amount"))
            }
            ExportFormat::Csv => assert!(
                std::fs::read_to_string(&path)
                    .unwrap()
                    .contains("id,amount,email,missing")
            ),
            _ => unreachable!(),
        }
    }
    driver.disconnect(session).await.unwrap();
}

#[tokio::test]
async fn sqlite_query_error_preserves_the_previous_export() {
    let dir = tempfile::tempdir().unwrap();
    let (driver, session) = database(&dir.path().join("source.db")).await;
    let path = dir.path().join("previous.parquet");
    std::fs::write(&path, "previous").unwrap();
    let progress = export(
        driver.clone(),
        session,
        &path,
        ExportFormat::Parquet,
        "SELECT * FROM table_that_does_not_exist",
        false,
    )
    .await;
    assert_eq!(progress.state, ExportState::Failed);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "previous");
    assert!(!std::fs::read_dir(dir.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|ext| ext == "partial")
    }));
    driver.disconnect(session).await.unwrap();
}
