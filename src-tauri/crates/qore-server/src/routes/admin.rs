// SPDX-License-Identifier: BUSL-1.1

use axum::extract::State;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::controlplane::model::GrantLevel;
use crate::controlplane::AuthContext;
use crate::error::ApiError;
use crate::state::AppState;

fn require_admin(ctx: &AuthContext) -> Result<(), ApiError> {
    if ctx.is_admin() {
        Ok(())
    } else {
        Err(ApiError::forbidden("admin token required"))
    }
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    email: String,
    password: String,
    #[serde(default)]
    is_admin: bool,
}

pub async fn create_user(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&ctx)?;
    let user = state
        .control
        .create_user(&req.email, &req.password, req.is_admin)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(json!(user)))
}

pub async fn list_users(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&ctx)?;
    let users = state.control.list_users().await.map_err(ApiError::internal)?;
    Ok(Json(json!(users)))
}

#[derive(Deserialize)]
pub struct CreateRoleRequest {
    name: String,
}

pub async fn create_role(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<CreateRoleRequest>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&ctx)?;
    let role = state
        .control
        .create_role(&req.name)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(json!(role)))
}

#[derive(Deserialize)]
pub struct AssignRoleRequest {
    user_id: String,
    role_id: String,
}

pub async fn assign_role(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<AssignRoleRequest>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&ctx)?;
    state
        .control
        .assign_role(&req.user_id, &req.role_id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct GrantRequest {
    role_id: String,
    connection_id: String,
    level: String,
}

pub async fn grant_connection(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<GrantRequest>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&ctx)?;
    let level = GrantLevel::parse(&req.level)
        .ok_or_else(|| ApiError::bad_request("level must be 'read' or 'write'"))?;
    state
        .control
        .grant_connection(&req.role_id, &req.connection_id, level)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(json!({ "ok": true })))
}
