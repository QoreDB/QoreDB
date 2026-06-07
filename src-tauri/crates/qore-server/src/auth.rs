// SPDX-License-Identifier: BUSL-1.1

use axum::extract::{Request, State};
use axum::http::{header::AUTHORIZATION, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::controlplane::auth::verify_jwt;
use crate::controlplane::AuthContext;
use crate::state::AppState;

pub async fn require_token(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_bearer)
        .map(str::to_string);

    let Some(token) = token else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    let ctx = if constant_time_eq(token.as_bytes(), state.config.token.as_bytes()) {
        AuthContext::Admin
    } else if let Some(claims) = verify_jwt(&state.config.token, &token) {
        let grants = state
            .control
            .user_grants(&claims.sub)
            .await
            .unwrap_or_default();
        AuthContext::User { grants }
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

fn parse_bearer(header: &str) -> Option<&str> {
    let (scheme, rest) = header.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = rest.trim();
    (!token.is_empty()).then_some(token)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}
