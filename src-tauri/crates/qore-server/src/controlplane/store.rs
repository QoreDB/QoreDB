// SPDX-License-Identifier: BUSL-1.1

use std::collections::HashMap;
use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::controlplane::auth::hash_password;
use crate::controlplane::model::{GrantLevel, Role, User};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    pw_hash TEXT NOT NULL,
    is_admin INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS roles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS user_roles (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);
CREATE TABLE IF NOT EXISTS connection_grants (
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    connection_id TEXT NOT NULL,
    level TEXT NOT NULL,
    PRIMARY KEY (role_id, connection_id)
);
";

#[derive(Clone)]
pub struct ControlStore {
    pool: SqlitePool,
}

impl ControlStore {
    pub async fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::raw_sql(SCHEMA)
            .execute(&pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Self { pool })
    }

    pub async fn create_user(
        &self,
        email: &str,
        password: &str,
        is_admin: bool,
    ) -> Result<User, String> {
        let id = Uuid::new_v4().to_string();
        let hash = hash_password(password)?;
        sqlx::query("INSERT INTO users (id, email, pw_hash, is_admin) VALUES (?, ?, ?, ?)")
            .bind(&id)
            .bind(email)
            .bind(&hash)
            .bind(is_admin as i64)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(User {
            id,
            email: email.to_string(),
            is_admin,
        })
    }

    pub async fn find_user_by_email(&self, email: &str) -> Result<Option<(User, String)>, String> {
        let row = sqlx::query("SELECT id, email, pw_hash, is_admin FROM users WHERE email = ?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(row.map(|r| {
            (
                User {
                    id: r.get("id"),
                    email: r.get("email"),
                    is_admin: r.get::<i64, _>("is_admin") != 0,
                },
                r.get("pw_hash"),
            )
        }))
    }

    pub async fn list_users(&self) -> Result<Vec<User>, String> {
        let rows = sqlx::query("SELECT id, email, is_admin FROM users ORDER BY email")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .map(|r| User {
                id: r.get("id"),
                email: r.get("email"),
                is_admin: r.get::<i64, _>("is_admin") != 0,
            })
            .collect())
    }

    pub async fn count_users(&self) -> Result<i64, String> {
        let row = sqlx::query("SELECT COUNT(*) AS n FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(row.get("n"))
    }

    pub async fn create_role(&self, name: &str) -> Result<Role, String> {
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO roles (id, name) VALUES (?, ?)")
            .bind(&id)
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Role {
            id,
            name: name.to_string(),
        })
    }

    pub async fn assign_role(&self, user_id: &str, role_id: &str) -> Result<(), String> {
        sqlx::query("INSERT OR IGNORE INTO user_roles (user_id, role_id) VALUES (?, ?)")
            .bind(user_id)
            .bind(role_id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn grant_connection(
        &self,
        role_id: &str,
        connection_id: &str,
        level: GrantLevel,
    ) -> Result<(), String> {
        sqlx::query(
            "INSERT INTO connection_grants (role_id, connection_id, level) VALUES (?, ?, ?)
             ON CONFLICT(role_id, connection_id) DO UPDATE SET level = excluded.level",
        )
        .bind(role_id)
        .bind(connection_id)
        .bind(level.as_str())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Effective grants for a user across all its roles. When several roles
    /// grant the same connection, the strongest level wins.
    pub async fn user_grants(&self, user_id: &str) -> Result<HashMap<String, GrantLevel>, String> {
        let rows = sqlx::query(
            "SELECT g.connection_id AS connection_id, g.level AS level
             FROM connection_grants g
             JOIN user_roles ur ON ur.role_id = g.role_id
             WHERE ur.user_id = ?",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        let mut grants: HashMap<String, GrantLevel> = HashMap::new();
        for r in rows {
            let conn: String = r.get("connection_id");
            let level_str: String = r.get("level");
            let Some(level) = GrantLevel::parse(&level_str) else {
                continue;
            };
            grants
                .entry(conn)
                .and_modify(|existing| *existing = existing.max(level))
                .or_insert(level);
        }
        Ok(grants)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controlplane::auth::verify_password;
    use tempfile::TempDir;

    async fn store() -> (TempDir, ControlStore) {
        let dir = TempDir::new().unwrap();
        let store = ControlStore::open(&dir.path().join("control.db"))
            .await
            .unwrap();
        (dir, store)
    }

    #[tokio::test]
    async fn user_and_grant_resolution() {
        let (_dir, store) = store().await;

        let user = store.create_user("a@b.c", "pw", false).await.unwrap();
        let (found, hash) = store.find_user_by_email("a@b.c").await.unwrap().unwrap();
        assert_eq!(found.id, user.id);
        assert!(verify_password("pw", &hash));
        assert!(!verify_password("nope", &hash));

        let role = store.create_role("analysts").await.unwrap();
        store.assign_role(&user.id, &role.id).await.unwrap();
        store
            .grant_connection(&role.id, "conn_1", GrantLevel::Read)
            .await
            .unwrap();

        let grants = store.user_grants(&user.id).await.unwrap();
        assert_eq!(grants.get("conn_1"), Some(&GrantLevel::Read));
        assert_eq!(grants.get("conn_2"), None);
    }

    #[tokio::test]
    async fn write_supersedes_read_across_roles() {
        let (_dir, store) = store().await;
        let user = store.create_user("a@b.c", "pw", false).await.unwrap();
        let r1 = store.create_role("r1").await.unwrap();
        let r2 = store.create_role("r2").await.unwrap();
        store.assign_role(&user.id, &r1.id).await.unwrap();
        store.assign_role(&user.id, &r2.id).await.unwrap();
        store
            .grant_connection(&r1.id, "conn_1", GrantLevel::Read)
            .await
            .unwrap();
        store
            .grant_connection(&r2.id, "conn_1", GrantLevel::Write)
            .await
            .unwrap();

        let grants = store.user_grants(&user.id).await.unwrap();
        assert_eq!(grants.get("conn_1"), Some(&GrantLevel::Write));
    }

    #[tokio::test]
    async fn count_users_tracks_inserts() {
        let (_dir, store) = store().await;
        assert_eq!(store.count_users().await.unwrap(), 0);
        store.create_user("a@b.c", "pw", true).await.unwrap();
        assert_eq!(store.count_users().await.unwrap(), 1);
    }
}
