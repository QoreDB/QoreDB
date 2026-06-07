// SPDX-License-Identifier: BUSL-1.1

use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use crate::controlplane::auth::issue_jwt;
use crate::controlplane::oidc::random_token;
use crate::error::ApiError;
use crate::state::AppState;

pub async fn start(State(state): State<AppState>) -> Result<Redirect, ApiError> {
    let Some(oidc) = state.oidc.as_ref() else {
        return Err(ApiError::bad_request("OIDC is not configured"));
    };
    let url = oidc.start().map_err(ApiError::internal)?;
    Ok(Redirect::to(&url))
}

#[derive(Deserialize)]
pub struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// IdP redirect target. On success it mints our own JWT (JIT-provisioning the
/// user as a non-admin with no grants on first login) and bounces the browser
/// back to the SPA with `?sso_token=`; failures bounce with `?sso_error=`.
pub async fn callback(State(st): State<AppState>, Query(p): Query<CallbackParams>) -> Response {
    let Some(oidc) = st.oidc.as_ref() else {
        return ApiError::bad_request("OIDC is not configured").into_response();
    };
    if p.error.is_some() {
        return Redirect::to("/?sso_error=idp_error").into_response();
    }
    let (Some(code), Some(state)) = (p.code, p.state) else {
        return Redirect::to("/?sso_error=invalid_callback").into_response();
    };

    let email = match oidc.callback(&code, &state).await {
        Ok(email) => email,
        Err(e) => {
            tracing::warn!(error = %e, "OIDC callback failed");
            return Redirect::to("/?sso_error=auth_failed").into_response();
        }
    };

    let user = match st.control.find_user_by_email(&email).await {
        Ok(Some((user, _))) => user,
        Ok(None) => match st.control.create_user(&email, &random_token(32), false).await {
            Ok(user) => user,
            Err(e) => {
                tracing::error!(error = %e, "OIDC JIT provisioning failed");
                return Redirect::to("/?sso_error=provisioning_failed").into_response();
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "OIDC user lookup failed");
            return Redirect::to("/?sso_error=server_error").into_response();
        }
    };

    match issue_jwt(&st.config.token, &user.id, &user.email) {
        Ok(token) => Redirect::to(&format!("/?sso_token={token}")).into_response(),
        Err(_) => Redirect::to("/?sso_error=server_error").into_response(),
    }
}
