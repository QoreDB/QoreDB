// SPDX-License-Identifier: BUSL-1.1

use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::controlplane::auth::{issue_jwt, verify_password};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LoginRequest {
    email: String,
    password: String,
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<Value>, ApiError> {
    let found = state
        .control
        .find_user_by_email(&req.email)
        .await
        .map_err(ApiError::internal)?;

    let Some((user, hash)) = found else {
        return Err(ApiError::unauthorized("invalid credentials"));
    };
    if !verify_password(&req.password, &hash) {
        return Err(ApiError::unauthorized("invalid credentials"));
    }

    let token = issue_jwt(&state.config.token, &user.id, &user.email).map_err(ApiError::internal)?;
    Ok(Json(json!({
        "token": token,
        "email": user.email,
        "isAdmin": user.is_admin,
    })))
}

/// Bootstrap registration: allowed only while the instance has no users, where
/// it creates the first admin. Once any user exists it is closed (403) and
/// further accounts are created through the admin API.
pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<Value>, ApiError> {
    let count = state.control.count_users().await.map_err(ApiError::internal)?;
    if count > 0 {
        return Err(ApiError::forbidden("registration is closed"));
    }
    let user = state
        .control
        .create_user(&req.email, &req.password, true)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(json!({ "email": user.email, "isAdmin": user.is_admin })))
}

/// Public setup probe: tells the web whether the first admin still needs to be
/// created (`setupRequired`) so the UI can route to register vs login.
pub async fn status(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let count = state.control.count_users().await.map_err(ApiError::internal)?;
    Ok(Json(json!({
        "setupRequired": count == 0,
        "ssoEnabled": state.oidc.is_some(),
    })))
}
