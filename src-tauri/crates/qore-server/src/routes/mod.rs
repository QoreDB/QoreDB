// SPDX-License-Identifier: BUSL-1.1

mod bridge;
mod stream;

use axum::routing::{get, post};
use axum::{Json, Router};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/status", get(status))
        .route("/api/invoke", post(bridge::invoke))
        .route("/api/stream/execute_query", post(stream::execute_query))
}

async fn status() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "running": true,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
