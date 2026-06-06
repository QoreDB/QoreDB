// SPDX-License-Identifier: BUSL-1.1

use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use qore_core::{Namespace, StreamEvent};

use crate::config::QUERY_TIMEOUT_MS;
use crate::error::ApiError;
use crate::session::parse_session;
use crate::state::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamBody {
    pub session_id: String,
    pub query: String,
    #[serde(default)]
    pub namespace: Option<Namespace>,
    #[serde(default)]
    pub acknowledged_dangerous: bool,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub bypass_limits: bool,
}

pub async fn execute_query(
    State(state): State<AppState>,
    Json(body): Json<StreamBody>,
) -> Result<Response, ApiError> {
    let session = parse_session(&body.session_id).map_err(ApiError::bad_request)?;
    let ctx = state.ctx.clone();

    let pf = qore_service::query::preflight(
        &ctx.session_manager,
        &ctx.query_rate_limiter,
        &ctx.interceptor,
        &ctx.policy,
        session,
        &body.session_id,
        &body.query,
        body.namespace.as_ref(),
        body.acknowledged_dangerous,
    )
    .await
    .map_err(ApiError::bad_request)?;

    let query_id = ctx.query_manager.register(session).await;

    let (tx, rx) = tokio::sync::mpsc::channel::<StreamEvent>(256);
    let driver = pf.driver;
    let context = pf.context;
    let is_mutation = pf.is_mutation;
    let connection_key = pf.connection_key;
    let safety_warning = pf.safety_warning;
    let namespace = body.namespace;
    let query = body.query;
    let timeout = body.timeout_ms.unwrap_or(QUERY_TIMEOUT_MS);
    let bypass_limits = body.bypass_limits;

    tokio::spawn(async move {
        let _ = qore_service::query::execute(
            &ctx.query_manager,
            &ctx.query_cache,
            &ctx.interceptor,
            &ctx.policy,
            driver,
            &context,
            session,
            namespace,
            &query,
            query_id,
            is_mutation,
            connection_key.as_deref(),
            safety_warning.as_deref(),
            Some(timeout),
            bypass_limits,
            None,
            Some(tx),
            |_, _| {},
        )
        .await;
    });

    let stream = ReceiverStream::new(rx).map(|event| {
        let (name, data) = match event {
            StreamEvent::Columns(columns) => ("columns", json!(columns)),
            StreamEvent::Row(row) => ("row", json!(row)),
            StreamEvent::RowBatch(rows) => ("rows", json!(rows)),
            StreamEvent::Error(message) => ("error", json!(message)),
            StreamEvent::Done(affected) => ("done", json!(affected)),
        };
        Ok::<_, std::convert::Infallible>(Event::default().event(name).data(data.to_string()))
    });

    Ok(Sse::new(stream).into_response())
}
