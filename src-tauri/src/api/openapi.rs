// SPDX-License-Identifier: BUSL-1.1

//! OpenAPI 3.1 generator for the Instant Data API.
//!
//! The document is built from the live [`EndpointStore`] on every request, so
//! it always reflects the current registry. The generator stays intentionally
//! minimal: paths, parameters, verified response schemas, and a single
//! `bearerAuth` security scheme.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;
use serde_json::{json, Map, Value};

use super::auth::{parse_bearer, verify_token};
use super::endpoints::EndpointStore;
use super::handlers::ApiState;
use super::types::{EndpointParam, EndpointParamType, QueryShape};

/// Static document version. Bumped only when the generator's output shape
/// changes (separate from the app version).
const OPENAPI_VERSION: &str = "3.1.0";
const DOC_TITLE: &str = "QoreDB Instant Data API";
const DOC_VERSION: &str = "1";

/// `GET /openapi.json` handler. Requires a bearer that matches **any**
/// registered endpoint hash — the document is metadata-only but is gated
/// the same way as the data endpoints (loopback-only is the primary defense).
pub async fn handle_openapi(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_bearer)
        .ok_or((StatusCode::UNAUTHORIZED, "missing bearer"))?;
    if !any_endpoint_accepts(&state.store, token) {
        return Err((StatusCode::FORBIDDEN, "token rejected"));
    }
    Ok(Json(build_document(
        &state.store,
        state.openapi_base_url.get().map(String::as_str),
    )))
}

/// Returns `true` iff `token` matches the Argon2 hash of at least one
/// endpoint. Verification is constant-time per attempt; the loop is fine for
/// the small registries the app surfaces.
fn any_endpoint_accepts(store: &Arc<EndpointStore>, token: &str) -> bool {
    for meta in store.list() {
        if let Some(endpoint) = store.get_by_name(&meta.name) {
            if verify_token(token, &endpoint.token_hash).is_ok() {
                return true;
            }
        }
    }
    false
}

#[derive(Serialize)]
struct HealthPayload {
    status: &'static str,
    uptime_s: u64,
}

/// `GET /health` handler. Public (no bearer). Returns `{ status, uptime_s }`.
pub async fn handle_health(State(state): State<ApiState>) -> impl IntoResponse {
    Json(HealthPayload {
        status: "ok",
        uptime_s: state.started_at.elapsed().as_secs(),
    })
}

/// Generates the OpenAPI 3.1 document from the current endpoint registry.
/// Public so the Tauri command layer can serve the preview without going
/// through the local HTTP server.
pub fn build_document(store: &Arc<EndpointStore>, base_url: Option<&str>) -> Value {
    let mut paths = Map::new();

    // /health is documented but unauthenticated.
    paths.insert("/health".to_string(), health_path());

    for meta in store.list() {
        let Some(endpoint) = store.get_by_name(&meta.name) else {
            continue;
        };
        let path = format!("/api/{}", endpoint.name);
        paths.insert(
            path,
            endpoint_path(&endpoint.name, endpoint.shape, &endpoint.params),
        );
    }

    let mut document = json!({
        "openapi": OPENAPI_VERSION,
        "info": {
            "title": DOC_TITLE,
            "version": DOC_VERSION,
            "description": "Locally-hosted, read-only REST endpoints generated from parameterized SQL queries. Loopback-only.",
        },
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "Per-endpoint token issued at creation time.",
                }
            }
        },
        "security": [{ "bearerAuth": [] }],
        "paths": paths,
    });
    if let Some(base_url) = base_url {
        document
            .as_object_mut()
            .expect("OpenAPI document is an object")
            .insert(
                "servers".to_string(),
                json!([{
                    "url": base_url,
                    "description": "Running local Instant Data API server",
                }]),
            );
    }
    document
}

fn health_path() -> Value {
    json!({
        "get": {
            "summary": "Server health probe",
            "security": [],
            "responses": {
                "200": {
                    "description": "Server is up",
                    "content": {
                        "application/json": {
                            "schema": {
                                "type": "object",
                                "properties": {
                                    "status": { "type": "string" },
                                    "uptime_s": { "type": "integer", "format": "int64" }
                                },
                                "required": ["status", "uptime_s"]
                            }
                        }
                    }
                }
            }
        }
    })
}

fn endpoint_path(name: &str, shape: QueryShape, params: &[EndpointParam]) -> Value {
    let parameters: Vec<Value> = params.iter().map(parameter_object).collect();

    let response_schema = match shape {
        QueryShape::Rows => json!({
            "type": "object",
            "properties": {
                "data": { "type": "array", "items": { "type": "object" } },
                "count": { "type": "integer", "format": "int64", "minimum": 0 },
                "truncated": { "type": "boolean" }
            },
            "required": ["data", "count", "truncated"]
        }),
        QueryShape::Object => json!({
            "type": "object",
            "properties": {
                "data": {
                    "oneOf": [
                        { "type": "object" },
                        { "type": "null" }
                    ]
                }
            },
            "required": ["data"]
        }),
    };

    json!({
        "get": {
            "summary": format!("Run endpoint `{name}`"),
            "parameters": parameters,
            "responses": {
                "200": {
                    "description": "Query result",
                    "content": { "application/json": { "schema": response_schema } }
                },
                "401": { "description": "Missing or invalid bearer token" },
                "403": { "description": "Token rejected" },
                "429": { "description": "Rate limit exceeded" }
            }
        }
    })
}

fn parameter_object(p: &EndpointParam) -> Value {
    let schema_type = match p.kind {
        EndpointParamType::String => "string",
        EndpointParamType::Integer => "integer",
        EndpointParamType::Float => "number",
        EndpointParamType::Bool => "boolean",
    };
    let mut schema = Map::new();
    schema.insert("type".into(), Value::String(schema_type.into()));
    if let Some(d) = p.default.as_ref() {
        schema.insert("default".into(), Value::String(d.clone()));
    }
    json!({
        "name": p.name,
        "in": "query",
        "required": p.required,
        "schema": Value::Object(schema),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn store_with(endpoints: Vec<(&str, QueryShape, Vec<EndpointParam>)>) -> Arc<EndpointStore> {
        let tmp = TempDir::new().unwrap();
        let store = Arc::new(EndpointStore::new(tmp.path().to_path_buf()).unwrap());
        for (name, shape, params) in endpoints {
            store
                .create(
                    name.into(),
                    "conn-1".into(),
                    "SELECT 1".into(),
                    params,
                    shape,
                    100,
                    "hash".into(),
                )
                .unwrap();
        }
        // Leak the tempdir so the store stays usable for the assertion phase.
        // Tests are short-lived; the OS cleans up.
        std::mem::forget(tmp);
        store
    }

    #[test]
    fn document_includes_health_and_endpoints() {
        let store = store_with(vec![
            (
                "orders_top",
                QueryShape::Rows,
                vec![EndpointParam {
                    name: "limit".into(),
                    kind: EndpointParamType::Integer,
                    required: true,
                    default: None,
                }],
            ),
            ("single_user", QueryShape::Object, vec![]),
        ]);
        let doc = build_document(&store, Some("https://127.0.0.1:9443"));
        let paths = doc["paths"].as_object().unwrap();
        assert!(paths.contains_key("/health"));
        assert!(paths.contains_key("/api/orders_top"));
        assert!(paths.contains_key("/api/single_user"));
        assert_eq!(doc["openapi"], OPENAPI_VERSION);
        assert_eq!(doc["servers"][0]["url"], "https://127.0.0.1:9443");
    }

    #[test]
    fn rows_endpoint_has_no_undeclared_pagination_parameter() {
        let store = store_with(vec![("orders_top", QueryShape::Rows, vec![])]);
        let doc = build_document(&store, None);
        let params = doc["paths"]["/api/orders_top"]["get"]["parameters"]
            .as_array()
            .unwrap();
        assert!(params.iter().all(|p| p["name"] != "page"));
        assert!(doc.get("servers").is_none());
    }

    #[test]
    fn response_schemas_match_the_wire_envelopes() {
        let store = store_with(vec![
            ("rows", QueryShape::Rows, vec![]),
            ("single", QueryShape::Object, vec![]),
        ]);
        let doc = build_document(&store, None);

        let rows = &doc["paths"]["/api/rows"]["get"]["responses"]["200"]["content"]
            ["application/json"]["schema"];
        assert_eq!(rows["required"], json!(["data", "count", "truncated"]));
        assert_eq!(rows["properties"]["data"]["type"], "array");
        assert_eq!(rows["properties"]["count"]["type"], "integer");
        assert_eq!(rows["properties"]["truncated"]["type"], "boolean");
        assert!(rows["properties"].get("page").is_none());
        assert!(rows["properties"].get("total").is_none());

        let object = &doc["paths"]["/api/single"]["get"]["responses"]["200"]["content"]
            ["application/json"]["schema"];
        assert_eq!(object["required"], json!(["data"]));
        assert_eq!(object["properties"]["data"]["oneOf"][0]["type"], "object");
        assert_eq!(object["properties"]["data"]["oneOf"][1]["type"], "null");
    }

    #[test]
    fn typed_param_maps_to_openapi_type() {
        let store = store_with(vec![(
            "ep",
            QueryShape::Rows,
            vec![
                EndpointParam {
                    name: "s".into(),
                    kind: EndpointParamType::String,
                    required: true,
                    default: None,
                },
                EndpointParam {
                    name: "n".into(),
                    kind: EndpointParamType::Integer,
                    required: false,
                    default: Some("0".into()),
                },
                EndpointParam {
                    name: "f".into(),
                    kind: EndpointParamType::Float,
                    required: false,
                    default: None,
                },
                EndpointParam {
                    name: "b".into(),
                    kind: EndpointParamType::Bool,
                    required: false,
                    default: None,
                },
            ],
        )]);
        let doc = build_document(&store, None);
        let params = doc["paths"]["/api/ep"]["get"]["parameters"]
            .as_array()
            .unwrap();
        let by_name = |n: &str| params.iter().find(|p| p["name"] == n).unwrap();
        assert_eq!(by_name("s")["schema"]["type"], "string");
        assert_eq!(by_name("n")["schema"]["type"], "integer");
        assert_eq!(by_name("n")["schema"]["default"], "0");
        assert_eq!(by_name("f")["schema"]["type"], "number");
        assert_eq!(by_name("b")["schema"]["type"], "boolean");
    }
}
