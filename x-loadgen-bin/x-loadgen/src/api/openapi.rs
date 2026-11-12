//! Serves the hand-written `OpenAPI` 3 document.

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// Compiled-in `OpenAPI` 3 document.
pub const OPENAPI_JSON: &str = include_str!("../../openapi.json");

/// `GET /api/v1/openapi.json` (unauthenticated).
pub async fn openapi() -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    (StatusCode::OK, headers, OPENAPI_JSON).into_response()
}

/// Operations documented in [`OPENAPI_JSON`], as `(METHOD, path)`.
///
/// Used by the parse-and-list test to confirm the document covers every
/// router path.
#[cfg(test)]
#[must_use]
fn documented_operations() -> Vec<(String, String)> {
    let value: serde_json::Value =
        serde_json::from_str(OPENAPI_JSON).unwrap_or(serde_json::Value::Null);
    let Some(paths) = value.get("paths").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (path, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        for method in ["get", "post", "put", "patch", "delete"] {
            if item.contains_key(method) {
                out.push((method.to_ascii_uppercase(), path.clone()));
            }
        }
    }
    out.sort();
    out
}

/// Every authenticated or public API route registered on the router,
/// excluding the SPA fallback.
#[cfg(test)]
#[must_use]
fn expected_router_operations() -> Vec<(&'static str, &'static str)> {
    vec![
        ("GET", "/health"),
        ("GET", "/api/v1/openapi.json"),
        ("POST", "/api/v1/auth/login"),
        ("POST", "/api/v1/auth/logout"),
        ("POST", "/api/v1/auth/password"),
        ("GET", "/api/v1/auth/me"),
        ("GET", "/api/v1/meta"),
        ("GET", "/api/v1/collections"),
        ("POST", "/api/v1/collections"),
        ("GET", "/api/v1/collections/export"),
        ("POST", "/api/v1/collections/import"),
        ("GET", "/api/v1/collections/{id}"),
        ("PUT", "/api/v1/collections/{id}"),
        ("DELETE", "/api/v1/collections/{id}"),
        ("POST", "/api/v1/requests"),
        ("GET", "/api/v1/requests/{id}"),
        ("PUT", "/api/v1/requests/{id}"),
        ("DELETE", "/api/v1/requests/{id}"),
        ("GET", "/api/v1/environments"),
        ("POST", "/api/v1/environments"),
        ("GET", "/api/v1/environments/export"),
        ("POST", "/api/v1/environments/import"),
        ("GET", "/api/v1/environments/{id}"),
        ("PUT", "/api/v1/environments/{id}"),
        ("DELETE", "/api/v1/environments/{id}"),
        ("GET", "/api/v1/environments/{id}/tenants"),
        ("POST", "/api/v1/environments/{id}/tenants"),
        ("GET", "/api/v1/environments/{id}/tenants/{tid}"),
        ("PUT", "/api/v1/environments/{id}/tenants/{tid}"),
        ("DELETE", "/api/v1/environments/{id}/tenants/{tid}"),
        (
            "POST",
            "/api/v1/environments/{id}/tenants/{tid}/binding-keypair",
        ),
        ("GET", "/api/v1/load-profiles"),
        ("POST", "/api/v1/load-profiles"),
        ("GET", "/api/v1/load-profiles/{id}"),
        ("PUT", "/api/v1/load-profiles/{id}"),
        ("DELETE", "/api/v1/load-profiles/{id}"),
        ("POST", "/api/v1/probe"),
        ("POST", "/api/v1/crypto/binding-keypair"),
        ("GET", "/api/v1/runs"),
        ("POST", "/api/v1/runs"),
        ("GET", "/api/v1/runs/{id}"),
        ("DELETE", "/api/v1/runs/{id}"),
        ("GET", "/api/v1/runs/{id}/stats"),
        ("GET", "/api/v1/runs/{id}/timeseries"),
        ("GET", "/api/v1/runs/{id}/timeline"),
        ("GET", "/api/v1/runs/{id}/windows"),
        ("GET", "/api/v1/runs/{id}/verdict"),
        ("POST", "/api/v1/runs/{id}/stop"),
        ("POST", "/api/v1/runs/{id}/resume"),
        ("PATCH", "/api/v1/runs/{id}/traffic"),
        ("GET", "/api/v1/runs/{id}/events"),
        ("GET", "/api/v1/runs/{id}/report"),
        ("GET", "/api/v1/runs/{id}/report.json"),
        ("GET", "/api/v1/runs/{id}/report.html"),
        ("GET", "/api/v1/runs/{id}/samples"),
    ]
}

#[cfg(test)]
mod tests {
    use super::{OPENAPI_JSON, documented_operations, expected_router_operations};

    #[test]
    fn openapi_parses_and_lists_every_router_path() {
        let value: serde_json::Value = serde_json::from_str(OPENAPI_JSON).expect("parse");
        let version = value["openapi"].as_str().expect("version");
        assert!(
            version.starts_with("3."),
            "expected OpenAPI 3, got {version}"
        );
        assert!(
            value["info"]["title"]
                .as_str()
                .expect("title")
                .contains("x-loadgen")
        );
        assert!(value["components"]["securitySchemes"]["bearerAuth"].is_object());
        assert!(value["components"]["schemas"]["Error"].is_object());
        assert!(value["components"]["schemas"]["AssertSpec"].is_object());
        assert!(value["components"]["schemas"]["Verdict"].is_object());
        let documented = documented_operations();
        let mut expected: Vec<(String, String)> = expected_router_operations()
            .into_iter()
            .map(|(m, p)| (m.to_string(), p.to_string()))
            .collect();
        expected.sort();
        assert_eq!(documented, expected, "OpenAPI paths must match the router");
    }
}
