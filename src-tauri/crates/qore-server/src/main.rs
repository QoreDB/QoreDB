// SPDX-License-Identifier: BUSL-1.1

mod auth;
mod config;
mod controlplane;
mod error;
mod routes;
mod session;
mod state;

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{middleware, Router};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use qore_service::ServiceContext;

use config::ServerConfig;
use state::AppState;

async fn health() -> &'static str {
    "ok"
}

async fn serve_index(State(state): State<AppState>) -> Response {
    let Some(dir) = state.config.web_dir.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match tokio::fs::read_to_string(dir.join("index.html")).await {
        Ok(html) => Html(inject_web_flag(&html)).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn inject_web_flag(html: &str) -> String {
    let script = "<script>window.__QORE_WEB__=true;</script>";
    match html.find("</head>") {
        Some(pos) => format!("{}{}{}", &html[..pos], script, &html[pos..]),
        None => format!("{script}{html}"),
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = ServerConfig::from_env();
    if config.token_generated {
        tracing::warn!(token = %config.token, "QORE_SERVER_TOKEN not set — using generated token");
    }
    let addr = config.addr;
    let web_dir = config.web_dir.clone();

    let control = match controlplane::ControlStore::open(&config.config_dir.join("control.db")).await
    {
        Ok(store) => store,
        Err(e) => {
            tracing::error!(error = %e, "failed to open control database");
            std::process::exit(1);
        }
    };

    let state = AppState {
        ctx: Arc::new(ServiceContext::new()),
        config: Arc::new(config),
        control,
    };

    let protected = routes::router().layer(middleware::from_fn_with_state(
        state.clone(),
        auth::require_token,
    ));

    let mut app = Router::new()
        .route("/health", get(health))
        .merge(routes::public_router())
        .merge(protected);

    if let Some(dir) = web_dir {
        app = app
            .nest_service("/assets", ServeDir::new(dir.join("assets")))
            .fallback(serve_index);
    }

    let app = app.layer(CorsLayer::permissive()).with_state(state);

    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(%addr, error = %e, "failed to bind");
            std::process::exit(1);
        }
    };
    tracing::info!(%addr, "qore-server listening");

    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!(error = %e, "server stopped with error");
        std::process::exit(1);
    }
}
