// SPDX-License-Identifier: Apache-2.0

//! DuckDB-backed vector store for schema embeddings. Exact cosine scan via the
//! core `list_cosine_similarity` function — no VSS extension, no network.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use duckdb::{params, Connection};
use serde::Serialize;

pub const MIN_SCORE: f32 = 0.35;

#[derive(Debug, Clone)]
pub struct IndexedObject {
    pub object_id: String,
    pub kind: String,
    pub database: String,
    pub schema: Option<String>,
    pub table: String,
    pub column: Option<String>,
    pub document: String,
    pub fingerprint: String,
    pub sensitive: bool,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticHit {
    pub object_id: String,
    pub kind: String,
    pub database: String,
    pub schema: Option<String>,
    pub table: String,
    pub column: Option<String>,
    pub document: String,
    pub sensitive: bool,
    pub score: f32,
}

pub struct SemanticStore {
    conn: Mutex<Connection>,
}

const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS schema_objects (
    connection_key TEXT NOT NULL,
    object_id      TEXT NOT NULL,
    kind           TEXT NOT NULL,
    database_name  TEXT NOT NULL,
    schema_name    TEXT,
    table_name     TEXT NOT NULL,
    column_name    TEXT,
    document       TEXT NOT NULL,
    fingerprint    TEXT NOT NULL,
    sensitive      BOOLEAN NOT NULL DEFAULT FALSE,
    model          TEXT NOT NULL,
    dim            INTEGER NOT NULL,
    embedding      FLOAT[] NOT NULL,
    updated_at     TIMESTAMP NOT NULL DEFAULT now(),
    PRIMARY KEY (connection_key, object_id)
);
";

// duckdb-rs 1.4 cannot bind list parameters (panics in bind_parameter), so
// vectors are bound as text and cast to FLOAT[] in SQL. Debug formatting of
// f32 is shortest-round-trip, so no precision is lost.
fn embedding_to_text(embedding: &[f32]) -> String {
    format!("{embedding:?}")
}

impl SemanticStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create semantic index directory: {e}"))?;
        }
        let conn = Connection::open(path)
            .map_err(|e| format!("Failed to open semantic index: {e}"))?;
        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| format!("Failed to initialize semantic index: {e}"))?;
        conn.query_row(
            "SELECT list_cosine_similarity([1.0::FLOAT], [1.0::FLOAT])",
            [],
            |row| row.get::<_, f64>(0),
        )
        .map_err(|e| format!("Bundled DuckDB lacks list_cosine_similarity: {e}"))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    async fn with_conn<F, R>(self: &Arc<Self>, f: F) -> Result<R, String>
    where
        F: FnOnce(&Connection) -> Result<R, String> + Send + 'static,
        R: Send + 'static,
    {
        let store = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            let conn = store
                .conn
                .lock()
                .map_err(|e| format!("Failed to lock semantic index: {e}"))?;
            f(&conn)
        })
        .await
        .map_err(|e| format!("Semantic index task panicked: {e}"))?
    }

    pub async fn fingerprints(
        self: &Arc<Self>,
        connection_key: &str,
    ) -> Result<HashMap<String, String>, String> {
        let key = connection_key.to_string();
        self.with_conn(move |conn| {
            let mut stmt = conn
                .prepare("SELECT object_id, fingerprint FROM schema_objects WHERE connection_key = ?")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![key], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| e.to_string())?;
            let mut map = HashMap::new();
            for row in rows {
                let (id, fp) = row.map_err(|e| e.to_string())?;
                map.insert(id, fp);
            }
            Ok(map)
        })
        .await
    }

    pub async fn apply(
        self: &Arc<Self>,
        connection_key: &str,
        model: &str,
        upserts: Vec<IndexedObject>,
        deleted_ids: Vec<String>,
    ) -> Result<(), String> {
        let key = connection_key.to_string();
        let model = model.to_string();
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
            tx.execute(
                "DELETE FROM schema_objects WHERE connection_key = ? AND model != ?",
                params![key, model],
            )
            .map_err(|e| e.to_string())?;
            for id in &deleted_ids {
                tx.execute(
                    "DELETE FROM schema_objects WHERE connection_key = ? AND object_id = ?",
                    params![key, id],
                )
                .map_err(|e| e.to_string())?;
            }
            {
                let mut delete = tx
                    .prepare_cached(
                        "DELETE FROM schema_objects WHERE connection_key = ? AND object_id = ?",
                    )
                    .map_err(|e| e.to_string())?;
                let mut insert = tx
                    .prepare_cached(
                        "INSERT INTO schema_objects VALUES (?,?,?,?,?,?,?,?,?,?,?,?, CAST(? AS FLOAT[]), now())",
                    )
                    .map_err(|e| e.to_string())?;
                for obj in &upserts {
                    delete
                        .execute(params![key, obj.object_id])
                        .map_err(|e| e.to_string())?;
                    insert
                        .execute(params![
                            key,
                            obj.object_id,
                            obj.kind,
                            obj.database,
                            obj.schema,
                            obj.table,
                            obj.column,
                            obj.document,
                            obj.fingerprint,
                            obj.sensitive,
                            model,
                            obj.embedding.len() as i64,
                            embedding_to_text(&obj.embedding),
                        ])
                        .map_err(|e| e.to_string())?;
                }
            }
            tx.commit().map_err(|e| e.to_string())
        })
        .await
    }

    pub async fn search(
        self: &Arc<Self>,
        connection_key: &str,
        model: &str,
        query_vec: &[f32],
        limit: u32,
    ) -> Result<Vec<SemanticHit>, String> {
        let key = connection_key.to_string();
        let model = model.to_string();
        let dim = query_vec.len() as i64;
        let vec_text = embedding_to_text(query_vec);
        self.with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT object_id, kind, database_name, schema_name, table_name, column_name,
                            document, sensitive,
                            list_cosine_similarity(embedding, CAST(? AS FLOAT[])) AS score
                     FROM schema_objects
                     WHERE connection_key = ? AND model = ? AND dim = ?
                     ORDER BY score DESC
                     LIMIT ?",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![vec_text, key, model, dim, limit as i64], |row| {
                    Ok(SemanticHit {
                        object_id: row.get(0)?,
                        kind: row.get(1)?,
                        database: row.get(2)?,
                        schema: row.get(3)?,
                        table: row.get(4)?,
                        column: row.get(5)?,
                        document: row.get(6)?,
                        sensitive: row.get(7)?,
                        score: row.get::<_, f64>(8)? as f32,
                    })
                })
                .map_err(|e| e.to_string())?;
            let mut hits = Vec::new();
            for row in rows {
                let hit = row.map_err(|e| e.to_string())?;
                if hit.score >= MIN_SCORE {
                    hits.push(hit);
                }
            }
            Ok(hits)
        })
        .await
    }

    pub async fn count(self: &Arc<Self>, connection_key: &str, model: &str) -> Result<u64, String> {
        let key = connection_key.to_string();
        let model = model.to_string();
        self.with_conn(move |conn| {
            conn.query_row(
                "SELECT count(*) FROM schema_objects WHERE connection_key = ? AND model = ?",
                params![key, model],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n as u64)
            .map_err(|e| e.to_string())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(id: &str, table: &str, column: Option<&str>, embedding: Vec<f32>) -> IndexedObject {
        IndexedObject {
            object_id: id.to_string(),
            kind: if column.is_some() { "column" } else { "table" }.to_string(),
            database: "db".to_string(),
            schema: Some("public".to_string()),
            table: table.to_string(),
            column: column.map(String::from),
            document: format!("doc {id}"),
            fingerprint: format!("fp-{id}"),
            sensitive: false,
            embedding,
        }
    }

    #[tokio::test]
    async fn roundtrip_search_orders_by_cosine() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(SemanticStore::open(&dir.path().join("index.duckdb")).unwrap());

        store
            .apply(
                "conn1",
                "test-model",
                vec![
                    object("a", "customers", None, vec![1.0, 0.0, 0.0]),
                    object("b", "customers", Some("email"), vec![0.9, 0.1, 0.0]),
                    object("c", "orders", None, vec![0.0, 0.0, 1.0]),
                ],
                vec![],
            )
            .await
            .unwrap();

        let hits = store
            .search("conn1", "test-model", &[1.0, 0.0, 0.0], 10)
            .await
            .unwrap();
        assert_eq!(hits.len(), 2, "orthogonal vector must be filtered by MIN_SCORE");
        assert_eq!(hits[0].object_id, "a");
        assert_eq!(hits[1].object_id, "b");
        assert!(hits[0].score > hits[1].score);

        let other = store
            .search("conn2", "test-model", &[1.0, 0.0, 0.0], 10)
            .await
            .unwrap();
        assert!(other.is_empty());
    }

    #[tokio::test]
    async fn fingerprints_roundtrip_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(SemanticStore::open(&dir.path().join("index.duckdb")).unwrap());

        store
            .apply(
                "conn1",
                "test-model",
                vec![
                    object("a", "t1", None, vec![1.0, 0.0]),
                    object("b", "t2", None, vec![0.0, 1.0]),
                ],
                vec![],
            )
            .await
            .unwrap();

        let fps = store.fingerprints("conn1").await.unwrap();
        assert_eq!(fps.len(), 2);
        assert_eq!(fps.get("a").map(String::as_str), Some("fp-a"));

        store
            .apply("conn1", "test-model", vec![], vec!["a".to_string()])
            .await
            .unwrap();
        let fps = store.fingerprints("conn1").await.unwrap();
        assert_eq!(fps.len(), 1);
        assert_eq!(store.count("conn1", "test-model").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn model_change_purges_stale_rows() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(SemanticStore::open(&dir.path().join("index.duckdb")).unwrap());

        store
            .apply(
                "conn1",
                "old-model",
                vec![object("a", "t1", None, vec![1.0, 0.0])],
                vec![],
            )
            .await
            .unwrap();
        store
            .apply(
                "conn1",
                "new-model",
                vec![object("a", "t1", None, vec![1.0, 0.0])],
                vec![],
            )
            .await
            .unwrap();

        assert_eq!(store.count("conn1", "old-model").await.unwrap(), 0);
        assert_eq!(store.count("conn1", "new-model").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn upsert_replaces_existing_row() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(SemanticStore::open(&dir.path().join("index.duckdb")).unwrap());

        store
            .apply(
                "conn1",
                "m",
                vec![object("a", "t1", None, vec![1.0, 0.0])],
                vec![],
            )
            .await
            .unwrap();
        let mut updated = object("a", "t1", None, vec![0.0, 1.0]);
        updated.fingerprint = "fp-a2".to_string();
        store.apply("conn1", "m", vec![updated], vec![]).await.unwrap();

        let fps = store.fingerprints("conn1").await.unwrap();
        assert_eq!(fps.get("a").map(String::as_str), Some("fp-a2"));
        assert_eq!(store.count("conn1", "m").await.unwrap(), 1);
    }
}
