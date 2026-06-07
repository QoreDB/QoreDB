// SPDX-License-Identifier: BUSL-1.1

mod admin;
mod auth;
mod bridge;
mod stream;

use axum::routing::{get, post};
use axum::{Json, Router};

use crate::state::AppState;

/// Routes behind the auth middleware (admin token or user JWT).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/status", get(status))
        .route("/api/invoke", post(bridge::invoke))
        .route("/api/stream/execute_query", post(stream::execute_query))
        .route("/api/admin/users", get(admin::list_users).post(admin::create_user))
        .route("/api/admin/roles", post(admin::create_role))
        .route("/api/admin/assign", post(admin::assign_role))
        .route("/api/admin/grants", post(admin::grant_connection))
}

/// Routes reachable without authentication: setup probe, bootstrap register, and
/// login (exchanges credentials for a JWT).
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/status", get(auth::status))
        .route("/api/auth/register", post(auth::register))
        .route("/api/auth/login", post(auth::login))
}

async fn status() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "running": true,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
