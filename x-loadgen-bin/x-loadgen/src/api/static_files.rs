//! Serve the embedded Vue SPA, or a short message when it was not embedded.

#[cfg(frontend_embedded)]
use std::path::Path;

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};

#[cfg(frontend_embedded)]
#[derive(rust_embed::RustEmbed)]
#[folder = "web/dist"]
struct Assets;

/// Serve a packed asset, or fall back to `index.html` for in-app routes.
pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if is_reserved(path) {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    serve_asset(path)
}

fn is_reserved(path: &str) -> bool {
    path == "health" || path.starts_with("health/") || path == "api" || path.starts_with("api/")
}

fn serve_asset(path: &str) -> Response {
    #[cfg(frontend_embedded)]
    {
        let file = if path.is_empty() { "index.html" } else { path };
        if let Some(asset) = Assets::get(file) {
            return file_response(file, asset.data.as_ref());
        }
        if should_fallback(file)
            && let Some(index) = Assets::get("index.html")
        {
            return file_response("index.html", index.data.as_ref());
        }
        (StatusCode::NOT_FOUND, "not found").into_response()
    }
    #[cfg(not(frontend_embedded))]
    {
        let _ = path;
        not_embedded()
    }
}

#[cfg(frontend_embedded)]
fn should_fallback(path: &str) -> bool {
    Path::new(path).extension().is_none()
}

#[cfg(not(frontend_embedded))]
fn not_embedded() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "前端未嵌入。请先在 x-loadgen-bin/x-loadgen/web 执行 pnpm install && pnpm build，或不要设置 MAZBRI_SKIP_FRONTEND_EMBED。",
    )
        .into_response()
}

#[cfg(frontend_embedded)]
fn file_response(path: &str, body: &[u8]) -> Response {
    let mut response = body.to_vec().into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        content_type(path)
            .parse()
            .unwrap_or(header::HeaderValue::from_static("application/octet-stream")),
    );
    let cache = if path.ends_with("index.html") {
        "no-cache"
    } else {
        "public, max-age=31536000, immutable"
    };
    if let Ok(value) = header::HeaderValue::from_str(cache) {
        headers.insert(header::CACHE_CONTROL, value);
    }
    response
}

#[cfg(frontend_embedded)]
fn content_type(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("json" | "map") => "application/json",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
