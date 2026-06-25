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
use axum_server::tls_rustls::RustlsConfig;
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
        // Never write the full bearer token to logs (they may be shipped to
        // aggregators). Show only a short prefix for correlation and direct the
        // operator to set QORE_SERVER_TOKEN explicitly for a known value.
        let token_prefix: String = config.token.chars().take(8).collect();
        tracing::warn!(
            token_prefix = %token_prefix,
            "QORE_SERVER_TOKEN not set — using a randomly generated token (prefix shown). Set QORE_SERVER_TOKEN to use a known value."
        );
    }
    let addr = config.addr;
    let web_dir = config.web_dir.clone();
    let tls = match (config.tls_cert.clone(), config.tls_key.clone()) {
        (Some(cert), Some(key)) => Some((cert, key)),
        (None, None) => None,
        _ => {
            tracing::error!("QORE_SERVER_TLS_CERT and QORE_SERVER_TLS_KEY must be set together");
            std::process::exit(1);
        }
    };

    let control =
        match controlplane::ControlStore::open(&config.config_dir.join("control.db")).await {
            Ok(store) => store,
            Err(e) => {
                tracing::error!(error = %e, "failed to open control database");
                std::process::exit(1);
            }
        };

    let oidc = match controlplane::OidcConfig::from_env() {
        Some(cfg) => match controlplane::OidcProvider::discover(cfg).await {
            Ok(provider) => {
                tracing::info!("OIDC/SSO enabled");
                Some(Arc::new(provider))
            }
            Err(e) => {
                tracing::error!(error = %e, "OIDC discovery failed — SSO disabled");
                None
            }
        },
        None => None,
    };

    let state = AppState {
        ctx: Arc::new(ServiceContext::new()),
        config: Arc::new(config),
        control,
        oidc,
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
        let serve_dir = ServeDir::new(&dir)
            .append_index_html_on_directories(false)
            .fallback(get(serve_index).with_state(state.clone()));
        app = app.fallback_service(serve_dir);
    }

    let app = app.layer(CorsLayer::permissive()).with_state(state);

    if let Some((cert, key)) = tls {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let tls_config = match RustlsConfig::from_pem_file(&cert, &key).await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(cert = %cert.display(), key = %key.display(), error = %e, "failed to load TLS certificate");
                std::process::exit(1);
            }
        };
        tracing::info!(%addr, "qore-server listening (https)");
        if let Err(e) = axum_server::bind_rustls(addr, tls_config)
            .serve(app.into_make_service())
            .await
        {
            tracing::error!(error = %e, "server stopped with error");
            std::process::exit(1);
        }
        return;
    }

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
