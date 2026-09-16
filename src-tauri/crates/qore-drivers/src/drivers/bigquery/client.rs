// SPDX-License-Identifier: Apache-2.0

use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use qore_core::error::{EngineError, EngineResult};
use qore_core::types::ConnectionConfig;
use reqwest::header::HeaderMap;
use reqwest::{Client as HttpClient, StatusCode, Url};
use serde::{Deserialize, Serialize};

use super::response::{Param, QueryPage, TableData, TableInfo};
use crate::drivers::warehouse_compat::{KeyPair, build_https_client};

const API: &str = "https://bigquery.googleapis.com/bigquery/v2/";
const SCOPE: &str = "https://www.googleapis.com/auth/bigquery";
const TOKEN_LIFETIME: Duration = Duration::from_secs(3600);
const TOKEN_MARGIN: Duration = Duration::from_secs(5 * 60);
/// How long `jobs.query` may block before handing back an incomplete job.
/// Short on purpose: the job id is what makes `cancel` possible.
const FIRST_WAIT_MS: u32 = 2_000;
const POLL_WAIT_MS: u32 = 10_000;
pub const PAGE_SIZE: u32 = 10_000;

#[derive(Deserialize)]
struct ServiceAccount {
    project_id: String,
    private_key: String,
    client_email: String,
    #[serde(default = "default_token_uri")]
    token_uri: String,
}

fn default_token_uri() -> String {
    "https://oauth2.googleapis.com/token".to_string()
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    scope: &'static str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct JobRef {
    #[serde(rename = "projectId")]
    pub project_id: String,
    #[serde(rename = "jobId")]
    pub job_id: String,
    pub location: Option<String>,
}

pub struct QueryRequest<'a> {
    pub sql: &'a str,
    pub params: Vec<Param>,
    pub default_dataset: Option<(String, String)>,
    pub dry_run: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryBody<'a> {
    query: &'a str,
    use_legacy_sql: bool,
    timeout_ms: u32,
    max_results: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameter_mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    query_parameters: Vec<Param>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_dataset: Option<DatasetRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<&'a str>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    dry_run: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DatasetRef {
    project_id: String,
    dataset_id: String,
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: Option<ErrorBody>,
}

#[derive(Deserialize)]
struct ErrorBody {
    message: Option<String>,
    status: Option<String>,
}

pub struct BigQueryClient {
    http: HttpClient,
    api: Url,
    key: KeyPair,
    client_email: String,
    token_uri: String,
    /// Where the data lives and where the bill goes; the same project unless
    /// the connection says otherwise.
    pub project: String,
    pub billing_project: String,
    pub location: Option<String>,
    token: Mutex<Option<(String, Instant)>>,
}

impl BigQueryClient {
    pub fn new(config: &ConnectionConfig) -> EngineResult<Self> {
        let account: ServiceAccount =
            serde_json::from_str(config.password.trim()).map_err(|e| {
                EngineError::auth_failed(format!(
                    "The password must hold the service account JSON key: {e}"
                ))
            })?;
        let key = KeyPair::from_pem(&account.private_key)?;
        let option = |name: &str| {
            config
                .options
                .get(name)
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        };
        let project = config
            .database
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| account.project_id.clone());
        let billing_project = option("billing_project").unwrap_or_else(|| project.clone());
        let timeout = Duration::from_secs(config.pool_acquire_timeout_secs.unwrap_or(30) as u64)
            .max(Duration::from_secs(60));
        let http = build_https_client(timeout, config.ssl_ca_cert.as_deref(), HeaderMap::new())?;
        Ok(Self {
            http,
            api: Url::parse(API).expect("constant URL"),
            key,
            client_email: account.client_email,
            token_uri: account.token_uri,
            project,
            billing_project,
            location: option("location"),
            token: Mutex::new(None),
        })
    }

    #[cfg(test)]
    pub fn for_tests(base_url: &str) -> Self {
        Self {
            http: HttpClient::new(),
            api: Url::parse(&format!("{base_url}/bigquery/v2/")).unwrap(),
            key: KeyPair::from_pem(crate::drivers::warehouse_compat::TEST_PRIVATE_KEY).unwrap(),
            client_email: "svc@proj.iam.gserviceaccount.com".into(),
            token_uri: format!("{base_url}/token"),
            project: "proj".into(),
            billing_project: "bill".into(),
            location: Some("EU".into()),
            token: Mutex::new(None),
        }
    }

    async fn access_token(&self) -> EngineResult<String> {
        {
            let cache = self
                .token
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some((token, expires)) = cache.as_ref()
                && expires.saturating_duration_since(Instant::now()) > TOKEN_MARGIN
            {
                return Ok(token.clone());
            }
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| EngineError::internal(format!("System clock: {e}")))?
            .as_secs();
        let assertion = self.key.sign(&Claims {
            iss: &self.client_email,
            scope: SCOPE,
            aud: &self.token_uri,
            iat: now,
            exp: now + TOKEN_LIFETIME.as_secs(),
        })?;
        let response = self
            .http
            .post(&self.token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", assertion.as_str()),
            ])
            .send()
            .await
            .map_err(transport_error)?;
        let (status, text) = split(response).await?;
        if !status.is_success() {
            return Err(EngineError::auth_failed(format!(
                "Google refused the service account: {}",
                error_message(&text)
            )));
        }
        let token: TokenResponse = serde_json::from_str(&text)
            .map_err(|e| EngineError::auth_failed(format!("Unexpected token response: {e}")))?;
        let expires = Instant::now() + Duration::from_secs(token.expires_in);
        *self
            .token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            Some((token.access_token.clone(), expires));
        Ok(token.access_token)
    }

    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> EngineResult<reqwest::RequestBuilder> {
        let token = self.access_token().await?;
        let url = self
            .api
            .join(path)
            .map_err(|e| EngineError::internal(format!("Bad API path: {e}")))?;
        Ok(self.http.request(method, url).bearer_auth(token))
    }

    async fn get_text(&self, path: &str, query: &[(&str, String)]) -> EngineResult<String> {
        let response = self
            .request(reqwest::Method::GET, path)
            .await?
            .query(query)
            .send()
            .await
            .map_err(transport_error)?;
        let (status, text) = split(response).await?;
        if !status.is_success() {
            return Err(api_error(status, &text));
        }
        Ok(text)
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> EngineResult<T> {
        serde_json::from_str(&self.get_text(path, query).await?)
            .map_err(|e| EngineError::internal(format!("Unexpected BigQuery response: {e}")))
    }

    /// Starts a query. The page comes back complete for short queries; a
    /// longer one returns its job reference for `finish` to poll.
    pub async fn start(&self, request: QueryRequest<'_>) -> EngineResult<QueryPage> {
        let body = QueryBody {
            query: request.sql,
            use_legacy_sql: false,
            timeout_ms: FIRST_WAIT_MS,
            max_results: PAGE_SIZE,
            parameter_mode: (!request.params.is_empty()).then_some("POSITIONAL"),
            query_parameters: request.params,
            default_dataset: request
                .default_dataset
                .map(|(project_id, dataset_id)| DatasetRef {
                    project_id,
                    dataset_id,
                }),
            location: self.location.as_deref(),
            dry_run: request.dry_run,
        };
        let response = self
            .request(
                reqwest::Method::POST,
                &format!("projects/{}/queries", self.billing_project),
            )
            .await?
            .json(&body)
            .send()
            .await
            .map_err(transport_error)?;
        let (status, text) = split(response).await?;
        if !status.is_success() {
            return Err(api_error(status, &text));
        }
        QueryPage::parse(&text)
    }

    /// Polls the job to completion and walks every page after the first.
    pub async fn finish(&self, mut page: QueryPage, max_rows: usize) -> EngineResult<QueryPage> {
        let Some(job) = page.job.clone() else {
            page.finalize()?;
            if page.rows.len() > max_rows {
                return Err(EngineError::result_too_large(
                    page.rows.len() as u64,
                    max_rows as u64,
                ));
            }
            return Ok(page);
        };
        let path = format!("projects/{}/queries/{}", job.project_id, job.job_id);
        let mut location = Vec::new();
        if let Some(loc) = job.location.clone() {
            location.push(("location", loc));
        }
        while !page.complete {
            let mut query = location.clone();
            query.push(("timeoutMs", POLL_WAIT_MS.to_string()));
            query.push(("maxResults", PAGE_SIZE.to_string()));
            let next = QueryPage::parse(&self.get_text(&path, &query).await?)?;
            page.absorb(next)?;
        }
        while let Some(token) = page.page_token.take() {
            if page.rows.len() >= max_rows {
                return Err(EngineError::result_too_large(
                    page.total_rows.unwrap_or(page.rows.len() as u64 + 1),
                    max_rows as u64,
                ));
            }
            let mut query = location.clone();
            query.push(("pageToken", token));
            query.push(("maxResults", PAGE_SIZE.to_string()));
            let next = QueryPage::parse(&self.get_text(&path, &query).await?)?;
            page.absorb(next)?;
        }
        page.finalize()?;
        if page.rows.len() > max_rows {
            return Err(EngineError::result_too_large(
                page.rows.len() as u64,
                max_rows as u64,
            ));
        }
        Ok(page)
    }

    pub async fn cancel(&self, job: &JobRef) -> EngineResult<()> {
        let mut builder = self
            .request(
                reqwest::Method::POST,
                &format!("projects/{}/jobs/{}/cancel", job.project_id, job.job_id),
            )
            .await?;
        if let Some(location) = &job.location {
            builder = builder.query(&[("location", location)]);
        }
        let response = builder.send().await.map_err(transport_error)?;
        let (status, text) = split(response).await?;
        if status.is_success() {
            Ok(())
        } else {
            Err(api_error(status, &text))
        }
    }

    pub async fn list_projects(&self) -> EngineResult<Vec<String>> {
        #[derive(Deserialize)]
        struct Projects {
            #[serde(default)]
            projects: Vec<Project>,
        }
        #[derive(Deserialize)]
        struct Project {
            id: String,
        }
        let projects: Projects = self
            .get_json("projects", &[("maxResults", "200".to_string())])
            .await?;
        Ok(projects.projects.into_iter().map(|p| p.id).collect())
    }

    pub async fn list_datasets(&self, project: &str) -> EngineResult<Vec<String>> {
        #[derive(Deserialize)]
        struct Datasets {
            #[serde(default)]
            datasets: Vec<Dataset>,
            #[serde(rename = "nextPageToken")]
            next_page_token: Option<String>,
        }
        #[derive(Deserialize)]
        struct Dataset {
            #[serde(rename = "datasetReference")]
            reference: DatasetRef,
        }
        let path = format!("projects/{project}/datasets");
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut query = vec![("maxResults", "1000".to_string())];
            if let Some(t) = token.take() {
                query.push(("pageToken", t));
            }
            let page: Datasets = self.get_json(&path, &query).await?;
            out.extend(page.datasets.into_iter().map(|d| d.reference.dataset_id));
            match page.next_page_token {
                Some(t) => token = Some(t),
                None => break,
            }
        }
        Ok(out)
    }

    pub async fn list_tables(
        &self,
        project: &str,
        dataset: &str,
    ) -> EngineResult<Vec<(String, String)>> {
        #[derive(Deserialize)]
        struct Tables {
            #[serde(default)]
            tables: Vec<Table>,
            #[serde(rename = "nextPageToken")]
            next_page_token: Option<String>,
        }
        #[derive(Deserialize)]
        struct Table {
            #[serde(rename = "tableReference")]
            reference: TableRef,
            #[serde(rename = "type", default)]
            kind: String,
        }
        #[derive(Deserialize)]
        struct TableRef {
            #[serde(rename = "tableId")]
            table_id: String,
        }
        let path = format!("projects/{project}/datasets/{dataset}/tables");
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut query = vec![("maxResults", "1000".to_string())];
            if let Some(t) = token.take() {
                query.push(("pageToken", t));
            }
            let page: Tables = self.get_json(&path, &query).await?;
            out.extend(
                page.tables
                    .into_iter()
                    .map(|t| (t.reference.table_id, t.kind)),
            );
            match page.next_page_token {
                Some(t) => token = Some(t),
                None => break,
            }
        }
        Ok(out)
    }

    pub async fn get_table(
        &self,
        project: &str,
        dataset: &str,
        table: &str,
    ) -> EngineResult<TableInfo> {
        self.get_json(
            &format!("projects/{project}/datasets/{dataset}/tables/{table}"),
            &[],
        )
        .await
    }

    /// `tabledata.list` reads rows straight from storage: no job, no bytes
    /// billed, no warehouse equivalent to wake up.
    pub async fn table_data(
        &self,
        project: &str,
        dataset: &str,
        table: &str,
        start_index: u64,
        max_results: u32,
    ) -> EngineResult<TableData> {
        self.get_json(
            &format!("projects/{project}/datasets/{dataset}/tables/{table}/data"),
            &[
                ("startIndex", start_index.to_string()),
                ("maxResults", max_results.to_string()),
            ],
        )
        .await
    }

    pub async fn create_dataset(&self, project: &str, dataset: &str) -> EngineResult<()> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body {
            dataset_reference: DatasetRef,
            #[serde(skip_serializing_if = "Option::is_none")]
            location: Option<String>,
        }
        let response = self
            .request(
                reqwest::Method::POST,
                &format!("projects/{project}/datasets"),
            )
            .await?
            .json(&Body {
                dataset_reference: DatasetRef {
                    project_id: project.to_string(),
                    dataset_id: dataset.to_string(),
                },
                location: self.location.clone(),
            })
            .send()
            .await
            .map_err(transport_error)?;
        let (status, text) = split(response).await?;
        if status.is_success() || status == StatusCode::CONFLICT {
            Ok(())
        } else {
            Err(api_error(status, &text))
        }
    }

    pub async fn delete_dataset(&self, project: &str, dataset: &str) -> EngineResult<()> {
        let response = self
            .request(
                reqwest::Method::DELETE,
                &format!("projects/{project}/datasets/{dataset}"),
            )
            .await?
            .query(&[("deleteContents", "true")])
            .send()
            .await
            .map_err(transport_error)?;
        let (status, text) = split(response).await?;
        if status.is_success() || status == StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(api_error(status, &text))
        }
    }
}

async fn split(response: reqwest::Response) -> EngineResult<(StatusCode, String)> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| EngineError::connection_failed(format!("BigQuery read body: {e}")))?;
    Ok((status, text))
}

fn transport_error(e: reqwest::Error) -> EngineError {
    if e.is_timeout() {
        EngineError::connection_failed("BigQuery request timed out")
    } else {
        EngineError::connection_failed(format!("BigQuery request failed: {e}"))
    }
}

fn error_message(text: &str) -> String {
    serde_json::from_str::<ErrorEnvelope>(text)
        .ok()
        .and_then(|e| e.error)
        .and_then(|e| e.message)
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| text.trim().to_string())
}

fn api_error(status: StatusCode, text: &str) -> EngineError {
    let message = error_message(text);
    let reason = serde_json::from_str::<ErrorEnvelope>(text)
        .ok()
        .and_then(|e| e.error)
        .and_then(|e| e.status)
        .unwrap_or_default();
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            EngineError::auth_failed(format!("BigQuery refused the request: {message}"))
        }
        _ if message.starts_with("Syntax error") => EngineError::syntax_error(message),
        _ if reason.is_empty() => EngineError::execution_error(message),
        _ => EngineError::execution_error(format!("{message} ({reason})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qore_core::types::Value;
    use wiremock::matchers::{body_string_contains, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn mock_token(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("jwt-bearer"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "ya29.token", "expires_in": 3599, "token_type": "Bearer"
            })))
            .expect(1)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn the_service_account_is_traded_once_for_a_bearer() {
        let server = MockServer::start().await;
        mock_token(&server).await;
        Mock::given(method("GET"))
            .and(path("/bigquery/v2/projects/proj/datasets"))
            .and(wiremock::matchers::header(
                "authorization",
                "Bearer ya29.token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "datasets": [{"datasetReference": {"projectId": "proj", "datasetId": "sales"}}]
            })))
            .expect(2)
            .mount(&server)
            .await;
        let client = BigQueryClient::for_tests(&server.uri());
        assert_eq!(client.list_datasets("proj").await.unwrap(), ["sales"]);
        assert_eq!(client.list_datasets("proj").await.unwrap(), ["sales"]);

        let token_request = &server.received_requests().await.unwrap()[0];
        let body = String::from_utf8(token_request.body.clone()).unwrap();
        let assertion = body.split("assertion=").nth(1).unwrap();
        let payload = assertion.split('.').nth(1).unwrap();
        let claims: serde_json::Value = serde_json::from_slice(
            &base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, payload)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(claims["iss"], "svc@proj.iam.gserviceaccount.com");
        assert_eq!(claims["scope"], SCOPE);
        assert!(claims["aud"].as_str().unwrap().ends_with("/token"));
    }

    #[tokio::test]
    async fn a_query_is_billed_to_the_billing_project_and_polled_to_completion() {
        let server = MockServer::start().await;
        mock_token(&server).await;
        Mock::given(method("POST"))
            .and(path("/bigquery/v2/projects/bill/queries"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jobComplete": false,
                "jobReference": {"projectId": "bill", "jobId": "job1", "location": "EU"}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/bigquery/v2/projects/bill/queries/job1"))
            .and(query_param("pageToken", "p2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jobComplete": true,
                "rows": [{"f": [{"v": "2"}, {"v": "b"}]}]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/bigquery/v2/projects/bill/queries/job1"))
            .and(query_param("location", "EU"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jobComplete": true,
                "schema": {"fields": [
                    {"name": "id", "type": "INTEGER", "mode": "REQUIRED"},
                    {"name": "name", "type": "STRING", "mode": "NULLABLE"}
                ]},
                "rows": [{"f": [{"v": "1"}, {"v": "a"}]}],
                "totalRows": "2",
                "pageToken": "p2",
                "totalBytesProcessed": "1024"
            })))
            .mount(&server)
            .await;

        let client = BigQueryClient::for_tests(&server.uri());
        let started = client
            .start(QueryRequest {
                sql: "SELECT id, name FROM t",
                params: vec![],
                default_dataset: Some(("proj".into(), "sales".into())),
                dry_run: false,
            })
            .await
            .unwrap();
        let page = client.finish(started, 1_000).await.unwrap();
        assert_eq!(page.columns.len(), 2);
        assert_eq!(page.rows.len(), 2);
        assert!(matches!(page.rows[1].values[0], Value::Int(2)));
        assert_eq!(page.total_bytes_processed, Some(1024));

        let query_request = &server.received_requests().await.unwrap()[1];
        let body: serde_json::Value = serde_json::from_slice(&query_request.body).unwrap();
        assert_eq!(body["useLegacySql"], false);
        assert_eq!(body["location"], "EU");
        assert_eq!(body["defaultDataset"]["datasetId"], "sales");
        assert!(body.get("dryRun").is_none());
        assert!(body.get("queryParameters").is_none());
    }

    #[tokio::test]
    async fn errors_map_by_status_and_message() {
        let server = MockServer::start().await;
        mock_token(&server).await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": {"code": 400, "status": "INVALID_ARGUMENT",
                          "message": "Syntax error: Unexpected identifier \"SELEC\" at [1:1]"}
            })))
            .mount(&server)
            .await;
        let client = BigQueryClient::for_tests(&server.uri());
        let Err(err) = client
            .start(QueryRequest {
                sql: "SELEC 1",
                params: vec![],
                default_dataset: None,
                dry_run: false,
            })
            .await
        else {
            panic!("a syntax error must be refused");
        };
        assert!(matches!(err, EngineError::SyntaxError { .. }), "{err:?}");
        assert!(matches!(
            api_error(StatusCode::FORBIDDEN, r#"{"error":{"message":"denied"}}"#),
            EngineError::AuthenticationFailed { .. }
        ));
    }

    #[tokio::test]
    async fn cancel_targets_the_job_in_its_location() {
        let server = MockServer::start().await;
        mock_token(&server).await;
        Mock::given(method("POST"))
            .and(path("/bigquery/v2/projects/bill/jobs/job1/cancel"))
            .and(query_param("location", "EU"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .expect(1)
            .mount(&server)
            .await;
        let client = BigQueryClient::for_tests(&server.uri());
        client
            .cancel(&JobRef {
                project_id: "bill".into(),
                job_id: "job1".into(),
                location: Some("EU".into()),
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn row_limit_rejects_unread_pages_but_accepts_complete_results() {
        let client = BigQueryClient::for_tests("http://127.0.0.1:1");
        for has_job in [false, true] {
            for (rows, more, should_fail) in [(1, false, false), (1, true, true), (2, false, true)]
            {
                if !has_job && more {
                    continue;
                }
                let mut payload = serde_json::json!({
                    "jobComplete": true,
                    "schema": {"fields": [{"name": "id", "type": "INTEGER"}]},
                    "rows": (0..rows).map(|i| serde_json::json!({"f": [{"v": i.to_string()}]})).collect::<Vec<_>>()
                });
                if has_job {
                    payload["jobReference"] =
                        serde_json::json!({"projectId": "proj", "jobId": "job"});
                }
                if more {
                    payload["pageToken"] = "next".into();
                    payload["totalRows"] = "2".into();
                }
                let result = client
                    .finish(QueryPage::parse(&payload.to_string()).unwrap(), 1)
                    .await;
                if should_fail {
                    assert!(matches!(
                        result,
                        Err(EngineError::ResultTooLarge { limit: 1, .. })
                    ));
                } else {
                    assert_eq!(result.unwrap().rows.len(), 1);
                }
            }
        }
    }
}
