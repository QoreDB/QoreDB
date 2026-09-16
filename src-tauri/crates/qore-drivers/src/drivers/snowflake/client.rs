// SPDX-License-Identifier: Apache-2.0

//! Snowflake SQL API v2 over HTTPS. Every statement is submitted
//! asynchronously and polled: that costs one extra round-trip, and buys a
//! statement handle from the first byte, so `cancel` always has something
//! to cancel.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use qore_core::error::{EngineError, EngineResult};
use qore_core::types::ConnectionConfig;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client as HttpClient, StatusCode, Url};
use serde::{Deserialize, Serialize};

use super::response::{Bindings, StatementBody};
use crate::drivers::warehouse_compat::{KeyPair, build_https_client};

pub const AUTH_KEY_PAIR: &str = "key_pair";
pub const AUTH_TOKEN: &str = "token";

/// Snowflake caps a key-pair JWT at one hour; re-minting a little early
/// keeps a long-running poll from straddling the expiry.
const JWT_LIFETIME: Duration = Duration::from_secs(3600);
const JWT_RENEW_AFTER: Duration = Duration::from_secs(50 * 60);
/// Server-side statement timeout, in seconds. A statement the user has not
/// cancelled by then is not interactive work.
const STATEMENT_TIMEOUT_SECS: u64 = 3600;
const POLL_INITIAL: Duration = Duration::from_millis(200);
const POLL_MAX: Duration = Duration::from_secs(2);

enum Auth {
    KeyPair(KeyPair),
    Token(String),
}

/// Per-request execution context. The SQL API keeps no session, so `USE`
/// does not stick: database, schema, warehouse and role travel with every
/// statement.
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub database: Option<String>,
    pub schema: Option<String>,
}

pub struct SnowflakeClient {
    http: HttpClient,
    base_url: Url,
    auth: Auth,
    /// Uppercased, region-less: what the JWT `iss` and `sub` claims want.
    account: String,
    user: String,
    pub warehouse: Option<String>,
    pub role: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    jwt: Mutex<Option<(String, Instant)>>,
}

#[derive(Serialize)]
struct Claims {
    iss: String,
    sub: String,
    iat: u64,
    exp: u64,
}

#[derive(Serialize)]
struct SubmitBody<'a> {
    statement: &'a str,
    timeout: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    database: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    schema: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warehouse: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bindings: Option<&'a Bindings>,
    parameters: BTreeMap<&'static str, &'static str>,
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "statementHandle")]
    statement_handle: Option<String>,
    code: Option<String>,
    message: Option<String>,
}

impl SnowflakeClient {
    pub fn new(config: &ConnectionConfig) -> EngineResult<Self> {
        let host = resolve_host(&config.host)?;
        let base_url = Url::parse(&format!("https://{host}/"))
            .map_err(|e| EngineError::connection_failed(format!("Invalid Snowflake host: {e}")))?;

        let auth = match option(config, "auth").as_deref().unwrap_or(AUTH_KEY_PAIR) {
            AUTH_KEY_PAIR => Auth::KeyPair(KeyPair::from_pem(&config.password)?),
            AUTH_TOKEN => {
                let token = config.password.trim();
                if token.is_empty() {
                    return Err(EngineError::auth_failed(
                        "A programmatic access token is required",
                    ));
                }
                Auth::Token(token.to_string())
            }
            other => {
                return Err(EngineError::validation(format!(
                    "Unknown Snowflake auth mode `{other}`"
                )));
            }
        };

        let timeout = Duration::from_secs(config.pool_acquire_timeout_secs.unwrap_or(30) as u64)
            .max(Duration::from_secs(60));
        let mut headers = HeaderMap::new();
        headers.insert(
            "User-Agent",
            HeaderValue::from_static(concat!("QoreDB/", env!("CARGO_PKG_VERSION"))),
        );
        let http = build_https_client(timeout, config.ssl_ca_cert.as_deref(), headers)?;

        Ok(Self {
            http,
            base_url,
            auth,
            account: account_for_jwt(&host),
            user: config.username.trim().to_ascii_uppercase(),
            warehouse: option(config, "warehouse"),
            role: option(config, "role"),
            database: config
                .database
                .as_deref()
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .map(str::to_owned),
            schema: option(config, "schema"),
            jwt: Mutex::new(None),
        })
    }

    #[cfg(test)]
    pub fn for_tests(base_url: &str, auth_mode: &str) -> Self {
        let auth = if auth_mode == AUTH_TOKEN {
            Auth::Token("pat-secret".into())
        } else {
            Auth::KeyPair(
                KeyPair::from_pem(crate::drivers::warehouse_compat::TEST_PRIVATE_KEY).unwrap(),
            )
        };
        Self {
            http: HttpClient::new(),
            base_url: Url::parse(base_url).unwrap(),
            auth,
            account: "MYORG-MYACCOUNT".into(),
            user: "ALICE".into(),
            warehouse: Some("COMPUTE_WH".into()),
            role: None,
            database: Some("DB".into()),
            schema: Some("PUBLIC".into()),
            jwt: Mutex::new(None),
        }
    }

    fn bearer(&self) -> EngineResult<(String, &'static str)> {
        match &self.auth {
            Auth::Token(token) => Ok((token.clone(), "PROGRAMMATIC_ACCESS_TOKEN")),
            Auth::KeyPair(key) => {
                let mut cache = self
                    .jwt
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some((token, minted)) = cache.as_ref()
                    && minted.elapsed() < JWT_RENEW_AFTER
                {
                    return Ok((token.clone(), "KEYPAIR_JWT"));
                }
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| EngineError::internal(format!("System clock: {e}")))?
                    .as_secs();
                let subject = format!("{}.{}", self.account, self.user);
                let token = key.sign(&Claims {
                    iss: format!("{subject}.{}", key.fingerprint),
                    sub: subject,
                    iat: now,
                    exp: now + JWT_LIFETIME.as_secs(),
                })?;
                *cache = Some((token.clone(), Instant::now()));
                Ok((token, "KEYPAIR_JWT"))
            }
        }
    }

    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> EngineResult<reqwest::RequestBuilder> {
        let (token, kind) = self.bearer()?;
        let url = self
            .base_url
            .join(path)
            .map_err(|e| EngineError::internal(format!("Bad API path: {e}")))?;
        Ok(self
            .http
            .request(method, url)
            .bearer_auth(token)
            .header("X-Snowflake-Authorization-Token-Type", kind))
    }

    /// Submits a statement and returns its handle without waiting for it.
    pub async fn submit(
        &self,
        sql: &str,
        bindings: Option<&Bindings>,
        context: &Context,
    ) -> EngineResult<String> {
        let body = SubmitBody {
            statement: sql,
            timeout: STATEMENT_TIMEOUT_SECS,
            database: context.database.as_deref().or(self.database.as_deref()),
            schema: context.schema.as_deref().or(self.schema.as_deref()),
            warehouse: self.warehouse.as_deref(),
            role: self.role.as_deref(),
            bindings,
            // One statement per request: the grid and the editor never batch,
            // and a second statement hidden after a `;` should not run.
            parameters: BTreeMap::from([("MULTI_STATEMENT_COUNT", "1")]),
        };
        let response = self
            .request(
                reqwest::Method::POST,
                "api/v2/statements?async=true&nullable=true",
            )?
            .json(&body)
            .send()
            .await
            .map_err(transport_error)?;
        let (status, text) = split(response).await?;
        if status != StatusCode::ACCEPTED && status != StatusCode::OK {
            return Err(api_error(status, &text));
        }
        let envelope: Envelope = serde_json::from_str(&text)
            .map_err(|e| EngineError::internal(format!("Unexpected Snowflake response: {e}")))?;
        envelope.statement_handle.ok_or_else(|| {
            EngineError::internal(format!(
                "Snowflake accepted the statement without a handle: {}",
                envelope.message.unwrap_or_default()
            ))
        })
    }

    /// Polls until the statement completes, then gathers every partition.
    pub async fn wait(&self, handle: &str, max_rows: usize) -> EngineResult<StatementBody> {
        let mut delay = POLL_INITIAL;
        let text = loop {
            let response = self
                .request(reqwest::Method::GET, &format!("api/v2/statements/{handle}"))?
                .send()
                .await
                .map_err(transport_error)?;
            let (status, text) = split(response).await?;
            match status {
                StatusCode::OK => break text,
                StatusCode::ACCEPTED => {
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(POLL_MAX);
                }
                _ => return Err(api_error(status, &text)),
            }
        };
        let mut body = StatementBody::parse(&text)?;
        for partition in 1..body.partitions {
            if body.rows.len() >= max_rows {
                return Err(EngineError::result_too_large(
                    body.rows.len() as u64 + 1,
                    max_rows as u64,
                ));
            }
            let response = self
                .request(
                    reqwest::Method::GET,
                    &format!("api/v2/statements/{handle}?partition={partition}"),
                )?
                .send()
                .await
                .map_err(transport_error)?;
            let (status, text) = split(response).await?;
            if status != StatusCode::OK {
                return Err(api_error(status, &text));
            }
            body.append_partition(&text)?;
        }
        if body.rows.len() > max_rows {
            return Err(EngineError::result_too_large(
                body.rows.len() as u64,
                max_rows as u64,
            ));
        }
        Ok(body)
    }

    pub async fn cancel(&self, handle: &str) -> EngineResult<()> {
        let response = self
            .request(
                reqwest::Method::POST,
                &format!("api/v2/statements/{handle}/cancel"),
            )?
            .send()
            .await
            .map_err(transport_error)?;
        let (status, text) = split(response).await?;
        if status.is_success() {
            Ok(())
        } else {
            Err(api_error(status, &text))
        }
    }
}

fn option(config: &ConnectionConfig, key: &str) -> Option<String> {
    config
        .options
        .get(key)
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

/// `myorg-myaccount` and `xy12345.eu-central-1` are account identifiers;
/// anything already ending in the Snowflake domain is taken as a host.
fn resolve_host(raw: &str) -> EngineResult<String> {
    let host = raw
        .trim()
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_ascii_lowercase();
    if host.is_empty() {
        return Err(EngineError::connection_failed(
            "A Snowflake account identifier is required",
        ));
    }
    if host
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
    {
        return Err(EngineError::connection_failed(format!(
            "`{raw}` is not a Snowflake account identifier or host"
        )));
    }
    Ok(
        if host.ends_with(".snowflakecomputing.com") || host.ends_with(".snowflakecomputing.cn") {
            host
        } else {
            format!("{host}.snowflakecomputing.com")
        },
    )
}

/// The JWT issuer wants the account locator or organisation-account pair,
/// uppercased, with any region or cloud segment removed.
fn account_for_jwt(host: &str) -> String {
    host.split('.').next().unwrap_or(host).to_ascii_uppercase()
}

async fn split(response: reqwest::Response) -> EngineResult<(StatusCode, String)> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| EngineError::connection_failed(format!("Snowflake read body: {e}")))?;
    Ok((status, text))
}

fn transport_error(e: reqwest::Error) -> EngineError {
    if e.is_timeout() {
        EngineError::connection_failed("Snowflake request timed out")
    } else {
        EngineError::connection_failed(format!("Snowflake request failed: {e}"))
    }
}

fn api_error(status: StatusCode, text: &str) -> EngineError {
    let envelope: Option<Envelope> = serde_json::from_str(text).ok();
    let message = envelope
        .as_ref()
        .and_then(|e| e.message.clone())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| text.trim().to_string());
    let code = envelope.and_then(|e| e.code).unwrap_or_default();
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            EngineError::auth_failed(format!("Snowflake refused the credentials: {message}"))
        }
        StatusCode::REQUEST_TIMEOUT => {
            EngineError::execution_error(format!("Snowflake statement timed out: {message}"))
        }
        _ if message.contains("SQL compilation error") => EngineError::syntax_error(message),
        _ => EngineError::execution_error(if code.is_empty() {
            message
        } else {
            format!("{message} (code {code})")
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn account_identifiers_become_hosts_and_jwt_accounts() {
        assert_eq!(
            resolve_host("myorg-myaccount").unwrap(),
            "myorg-myaccount.snowflakecomputing.com"
        );
        assert_eq!(
            resolve_host("XY12345.eu-central-1").unwrap(),
            "xy12345.eu-central-1.snowflakecomputing.com"
        );
        assert_eq!(
            resolve_host("https://acme-prod.privatelink.snowflakecomputing.com/").unwrap(),
            "acme-prod.privatelink.snowflakecomputing.com"
        );
        assert!(resolve_host("").is_err());
        assert!(resolve_host("bad host/with?stuff").is_err());

        assert_eq!(
            account_for_jwt("myorg-myaccount.snowflakecomputing.com"),
            "MYORG-MYACCOUNT"
        );
        assert_eq!(
            account_for_jwt("xy12345.eu-central-1.snowflakecomputing.com"),
            "XY12345"
        );
    }

    #[tokio::test]
    async fn a_key_pair_submission_sends_a_jwt_and_the_context() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/statements"))
            .and(query_param("async", "true"))
            .and(header(
                "X-Snowflake-Authorization-Token-Type",
                "KEYPAIR_JWT",
            ))
            .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
                "code": "333334",
                "statementHandle": "01b0-0000",
                "message": "Asynchronous execution in progress."
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = SnowflakeClient::for_tests(&server.uri(), AUTH_KEY_PAIR);
        let handle = client
            .submit("SELECT 1", None, &Context::default())
            .await
            .unwrap();
        assert_eq!(handle, "01b0-0000");

        let received = server.received_requests().await.unwrap();
        let request = &received[0];
        let auth = request
            .headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap();
        let jwt = auth.strip_prefix("Bearer ").unwrap();
        let payload = jwt.split('.').nth(1).unwrap();
        let claims: serde_json::Value = serde_json::from_slice(
            &base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, payload)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(claims["sub"], "MYORG-MYACCOUNT.ALICE");
        assert_eq!(
            claims["iss"],
            format!(
                "MYORG-MYACCOUNT.ALICE.{}",
                crate::drivers::warehouse_compat::TEST_FINGERPRINT
            )
        );
        assert_eq!(
            claims["exp"].as_u64().unwrap() - claims["iat"].as_u64().unwrap(),
            3600
        );

        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["statement"], "SELECT 1");
        assert_eq!(body["database"], "DB");
        assert_eq!(body["schema"], "PUBLIC");
        assert_eq!(body["warehouse"], "COMPUTE_WH");
        assert_eq!(body["parameters"]["MULTI_STATEMENT_COUNT"], "1");
        assert!(body.get("role").is_none());
    }

    #[tokio::test]
    async fn a_token_submission_uses_the_pat_header_and_the_cached_jwt_is_reused() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header(
                "X-Snowflake-Authorization-Token-Type",
                "PROGRAMMATIC_ACCESS_TOKEN",
            ))
            .and(header("authorization", "Bearer pat-secret"))
            .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
                "statementHandle": "h"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client = SnowflakeClient::for_tests(&server.uri(), AUTH_TOKEN);
        client
            .submit("SELECT 1", None, &Context::default())
            .await
            .unwrap();

        let key_client = SnowflakeClient::for_tests(&server.uri(), AUTH_KEY_PAIR);
        let (first, _) = key_client.bearer().unwrap();
        let (second, _) = key_client.bearer().unwrap();
        assert_eq!(first, second, "the JWT is minted once and reused");
    }

    #[tokio::test]
    async fn wait_polls_until_done_and_gathers_partitions() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/statements/h"))
            .and(query_param("partition", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [["3", "c"]]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v2/statements/h"))
            .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
                "code": "333334", "statementHandle": "h"
            })))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v2/statements/h"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "resultSetMetaData": {
                    "numRows": 3,
                    "format": "jsonv2",
                    "partitionInfo": [{"rowCount": 2}, {"rowCount": 1}],
                    "rowType": [
                        {"name": "ID", "type": "fixed", "nullable": false, "precision": 38, "scale": 0},
                        {"name": "NAME", "type": "text", "nullable": true}
                    ]
                },
                "data": [["1", "a"], ["2", "b"]],
                "statementHandle": "h",
                "code": "090001"
            })))
            .mount(&server)
            .await;

        let client = SnowflakeClient::for_tests(&server.uri(), AUTH_TOKEN);
        let body = client.wait("h", 10_000).await.unwrap();
        assert_eq!(body.columns.len(), 2);
        assert_eq!(body.rows.len(), 3);
        assert!(matches!(
            body.rows[2].values[0],
            qore_core::types::Value::Int(3)
        ));

        let polls = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.query().is_none())
            .count();
        assert_eq!(polls, 3, "two pending polls, then the result");
    }

    #[tokio::test]
    async fn api_errors_map_by_status_and_message() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
                "code": "001003",
                "message": "SQL compilation error: syntax error line 1 at position 7",
                "sqlState": "42000"
            })))
            .mount(&server)
            .await;
        let client = SnowflakeClient::for_tests(&server.uri(), AUTH_TOKEN);
        let err = client
            .submit("SELEC 1", None, &Context::default())
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::SyntaxError { .. }), "{err:?}");

        assert!(matches!(
            api_error(
                StatusCode::UNAUTHORIZED,
                r#"{"message":"JWT token is invalid."}"#
            ),
            EngineError::AuthenticationFailed { .. }
        ));
        assert!(matches!(
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "boom"),
            EngineError::ExecutionError { .. }
        ));
    }

    #[tokio::test]
    async fn cancel_posts_to_the_handle() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/statements/h/cancel"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": "090001", "message": "successfully canceled"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client = SnowflakeClient::for_tests(&server.uri(), AUTH_TOKEN);
        client.cancel("h").await.unwrap();
    }

    #[tokio::test]
    async fn row_limit_rejects_unread_partitions_but_accepts_complete_results() {
        for partitions in [1, 2] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v2/statements/h"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "resultSetMetaData": {
                        "partitionInfo": vec![serde_json::json!({"rowCount": 1}); partitions],
                        "rowType": [{"name": "ID", "type": "fixed", "scale": 0}]
                    },
                    "data": [["1"]]
                })))
                .expect(1)
                .mount(&server)
                .await;
            let client = SnowflakeClient::for_tests(&server.uri(), AUTH_TOKEN);
            let result = client.wait("h", 1).await;
            if partitions == 1 {
                assert_eq!(result.unwrap().rows.len(), 1);
            } else {
                assert!(matches!(
                    result,
                    Err(EngineError::ResultTooLarge { limit: 1, .. })
                ));
            }
        }
    }
}
