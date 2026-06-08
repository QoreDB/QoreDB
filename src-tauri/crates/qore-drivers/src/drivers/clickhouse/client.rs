// SPDX-License-Identifier: Apache-2.0

//! ClickHouse HTTP client.
//!
//! ClickHouse exposes two protocols: native binary (port 9000/9440) and HTTP
//! (port 8123/8443). We pick HTTP because it lets us drive arbitrary, runtime
//! queries with `FORMAT JSONCompactEachRowWithNamesAndTypes` — a single
//! response carries column names, types, and rows — without committing to a
//! row-typed Rust struct per query. That fits QoreDB's dynamic SQL workload.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use qore_core::error::{EngineError, EngineResult};
use qore_core::types::ConnectionConfig;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use reqwest::{Client as HttpClient, Url};
use uuid::Uuid;

/// Stateless ClickHouse client. Each instance owns one `reqwest::Client` so
/// that connection pooling and TLS reuse work across queries.
#[derive(Debug, Clone)]
pub(crate) struct ClickHouseClient {
    http: HttpClient,
    base_url: Url,
    user: String,
    password: String,
    default_database: String,
    active_database: Arc<RwLock<Option<String>>>,
    /// Distributed cluster name for DDL `ON CLUSTER` propagation. `None` means
    /// single-node behaviour (DDL applies locally only).
    cluster: Option<String>,
}

impl ClickHouseClient {
    pub fn new(config: &ConnectionConfig) -> EngineResult<Self> {
        // Refuse Basic-auth over cleartext HTTP because the base64 header is trivial to sniff
        // (audit B4-C8). Cleartext is only allowed when there is no password to leak.
        let ssl_disabled = !config.ssl
            && matches!(
                config.ssl_mode.as_deref(),
                None | Some("disable") | Some("allow")
            );
        if ssl_disabled && !config.password.is_empty() {
            return Err(EngineError::connection_failed(
                "ClickHouse: refusing to send password over cleartext HTTP. \
                 Enable TLS (ssl=true / ssl_mode=require) or remove the password.",
            ));
        }

        let scheme = if config.ssl { "https" } else { "http" };
        let host = if config.host.is_empty() {
            "localhost"
        } else {
            config.host.as_str()
        };
        let port = if config.port == 0 {
            if config.ssl {
                8443
            } else {
                8123
            }
        } else {
            config.port
        };

        let base_url = Url::parse(&format!("{scheme}://{host}:{port}/"))
            .map_err(|e| EngineError::connection_failed(format!("Invalid ClickHouse URL: {e}")))?;

        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );

        let timeout = Duration::from_secs(config.pool_acquire_timeout_secs.unwrap_or(30) as u64);

        let mut builder = HttpClient::builder()
            .default_headers(headers)
            .connect_timeout(Duration::from_secs(10))
            .timeout(timeout.max(Duration::from_secs(60)))
            .pool_idle_timeout(Duration::from_secs(90));

        if matches!(
            config.ssl_mode.as_deref(),
            Some("allow") | Some("prefer") | Some("disable")
        ) {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let http = builder.build().map_err(|e| {
            EngineError::connection_failed(format!("HTTP client build failed: {e}"))
        })?;

        let default_database = config
            .database
            .clone()
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| "default".to_string());

        let cluster = config
            .clickhouse_cluster
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);

        Ok(Self {
            http,
            base_url,
            user: config.username.clone(),
            password: config.password.clone(),
            default_database: default_database.clone(),
            active_database: Arc::new(RwLock::new(Some(default_database))),
            cluster,
        })
    }

    /// Returns the configured cluster name (validated, trimmed), if any.
    pub fn cluster(&self) -> Option<&str> {
        self.cluster.as_deref()
    }

    pub fn current_database(&self) -> String {
        match self.active_database.read() {
            Ok(guard) => guard
                .clone()
                .unwrap_or_else(|| self.default_database.clone()),
            Err(_) => self.default_database.clone(),
        }
    }

    pub fn set_current_database(&self, database: impl Into<String>) {
        if let Ok(mut guard) = self.active_database.write() {
            *guard = Some(database.into());
        }
    }

    /// Issue a query that returns no rows (DDL / mutations)
    pub async fn execute(&self, sql: &str, query_id: Option<&Uuid>) -> EngineResult<String> {
        self.execute_with_settings(sql, query_id, &[]).await
    }

    /// Like `execute` but lets the caller pass extra ClickHouse settings
    /// (e.g. `("mutations_sync", "2")`) via the URL query string. Used by
    /// `ALTER TABLE … UPDATE` to wait for mutation completion synchronously.
    pub async fn execute_with_settings(
        &self,
        sql: &str,
        query_id: Option<&Uuid>,
        settings: &[(&str, &str)],
    ) -> EngineResult<String> {
        let url = self.build_url_with_settings(
            query_id,
            Some(self.current_database().as_str()),
            settings,
        );
        let body = sql.to_owned();
        let resp = self
            .http
            .post(url)
            .basic_auth(&self.user, Some(&self.password))
            .body(body)
            .send()
            .await
            .map_err(|e| EngineError::execution_error(format!("ClickHouse send: {e}")))?;

        Self::ensure_ok(resp).await
    }

    /// Issue a query that streams JSON rows. Caller is responsible for parsing.
    /// Adds `FORMAT JSONCompactEachRowWithNamesAndTypes` if not already present.
    pub async fn fetch_json(&self, sql: &str, query_id: Option<&Uuid>) -> EngineResult<String> {
        self.fetch_json_with_params(sql, query_id, &[]).await
    }

    /// Like [`fetch_json`], but binds named parameters as `param_<name>` URL
    /// settings. Reference them in SQL with `{name:Type}` placeholders. This
    /// is the safe way to interpolate user-supplied strings (database name,
    /// table name, LIKE patterns) into a query — ClickHouse parses them as
    /// values rather than SQL fragments.
    pub async fn fetch_json_with_params(
        &self,
        sql: &str,
        query_id: Option<&Uuid>,
        params: &[(&str, &str)],
    ) -> EngineResult<String> {
        let with_format = ensure_format(sql);
        // ClickHouse reads bound params from `?param_<name>=` URL pairs.
        let param_pairs: Vec<(String, String)> = params
            .iter()
            .map(|(name, value)| (format!("param_{}", name), (*value).to_string()))
            .collect();
        let settings: Vec<(&str, &str)> = param_pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let url = self.build_url_with_settings(
            query_id,
            Some(self.current_database().as_str()),
            &settings,
        );
        let resp = self
            .http
            .post(url)
            .basic_auth(&self.user, Some(&self.password))
            .body(with_format)
            .send()
            .await
            .map_err(|e| EngineError::execution_error(format!("ClickHouse send: {e}")))?;

        Self::ensure_ok(resp).await
    }

    /// Best-effort cancellation: ClickHouse exposes `KILL QUERY WHERE
    /// query_id = <uuid>` — we issue it on a fresh request without targeting
    /// the running stream, so it does not need access to the running client.
    pub async fn kill_query(&self, query_id: &Uuid) -> EngineResult<()> {
        let sql = format!("KILL QUERY WHERE query_id = '{}' SYNC", query_id);
        let url = self.build_url(None, None);
        let _ = self
            .http
            .post(url)
            .basic_auth(&self.user, Some(&self.password))
            .body(sql)
            .send()
            .await
            .map_err(|e| EngineError::execution_error(format!("KILL QUERY send: {e}")))?;
        Ok(())
    }

    fn build_url(&self, query_id: Option<&Uuid>, database: Option<&str>) -> Url {
        self.build_url_with_settings(query_id, database, &[])
    }

    fn build_url_with_settings(
        &self,
        query_id: Option<&Uuid>,
        database: Option<&str>,
        settings: &[(&str, &str)],
    ) -> Url {
        let mut url = self.base_url.clone();
        {
            let mut q = url.query_pairs_mut();
            if let Some(id) = query_id {
                q.append_pair("query_id", &id.to_string());
            }
            if let Some(db) = database {
                q.append_pair("database", db);
            }
            // Pin the default format so parsers can rely on JSONCompactEachRowWithNamesAndTypes framing.
            q.append_pair("default_format", "JSONCompactEachRowWithNamesAndTypes");
            for (k, v) in settings {
                q.append_pair(k, v);
            }
        }
        url
    }

    async fn ensure_ok(resp: reqwest::Response) -> EngineResult<String> {
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| EngineError::execution_error(format!("ClickHouse read body: {e}")))?;
        if status.is_success() {
            Ok(body)
        } else {
            Err(EngineError::execution_error(format!(
                "ClickHouse {}: {}",
                status,
                body.trim()
            )))
        }
    }
}

fn ensure_format(sql: &str) -> String {
    let trimmed = sql.trim_end_matches(|c: char| c.is_whitespace() || c == ';');
    let upper = trimmed.to_ascii_uppercase();
    if upper.contains(" FORMAT ") {
        sql.to_string()
    } else {
        format!("{trimmed} FORMAT JSONCompactEachRowWithNamesAndTypes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(host: &str, port: u16, ssl: bool, db: Option<&str>) -> ConnectionConfig {
        cfg_with_cluster(host, port, ssl, db, None)
    }

    fn cfg_with_cluster(
        host: &str,
        port: u16,
        ssl: bool,
        db: Option<&str>,
        cluster: Option<&str>,
    ) -> ConnectionConfig {
        ConnectionConfig {
            driver: "clickhouse".into(),
            host: host.into(),
            port,
            username: "default".into(),
            password: "".into(),
            database: db.map(|s| s.to_string()),
            ssl,
            ssl_mode: None,
            environment: "development".into(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
            proxy: None,
            mssql_auth: None,
            clickhouse_cluster: cluster.map(|s| s.to_string()),
        }
    }

    #[test]
    fn picks_https_when_ssl_enabled() {
        let c = ClickHouseClient::new(&cfg("ch.example.com", 0, true, None)).unwrap();
        assert_eq!(c.base_url.scheme(), "https");
        assert_eq!(c.base_url.port_or_known_default(), Some(8443));
    }

    #[test]
    fn picks_http_default_port() {
        let c = ClickHouseClient::new(&cfg("localhost", 0, false, None)).unwrap();
        assert_eq!(c.base_url.scheme(), "http");
        assert_eq!(c.base_url.port_or_known_default(), Some(8123));
    }

    #[test]
    fn defaults_to_default_database_when_unset() {
        let c = ClickHouseClient::new(&cfg("localhost", 8123, false, None)).unwrap();
        assert_eq!(c.current_database(), "default");
    }

    #[test]
    fn honours_explicit_database() {
        let c = ClickHouseClient::new(&cfg("localhost", 8123, false, Some("metrics"))).unwrap();
        assert_eq!(c.current_database(), "metrics");
    }

    #[test]
    fn ensure_format_appends_when_missing() {
        let out = ensure_format("SELECT 1");
        assert!(out.ends_with(" FORMAT JSONCompactEachRowWithNamesAndTypes"));
    }

    #[test]
    fn ensure_format_keeps_user_specified_format() {
        let out = ensure_format("SELECT 1 FORMAT TabSeparated");
        assert_eq!(out, "SELECT 1 FORMAT TabSeparated");
    }

    #[test]
    fn ensure_format_strips_trailing_semicolon() {
        let out = ensure_format("SELECT 1;");
        assert!(out.starts_with("SELECT 1 FORMAT"));
        assert!(!out.contains(";"));
    }

    #[test]
    fn cluster_is_none_by_default() {
        let c = ClickHouseClient::new(&cfg("localhost", 8123, false, None)).unwrap();
        assert!(c.cluster().is_none());
    }

    #[test]
    fn cluster_is_captured_when_set() {
        let c = ClickHouseClient::new(&cfg_with_cluster(
            "localhost",
            8123,
            false,
            None,
            Some("shard_1"),
        ))
        .unwrap();
        assert_eq!(c.cluster(), Some("shard_1"));
    }

    #[test]
    fn cluster_blank_string_is_ignored() {
        let c = ClickHouseClient::new(&cfg_with_cluster(
            "localhost",
            8123,
            false,
            None,
            Some("   "),
        ))
        .unwrap();
        assert!(c.cluster().is_none());
    }
}
