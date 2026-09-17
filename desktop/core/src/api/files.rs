//! Plugin bundles and the SDK.
//!
//! A bundle is served from the plugin's own folder, the store entry or the
//! linked directory, with the CSP that makes the sandbox real: no network at all.
//! Scripts, styles and fonts only inline, from the bundle's own path, or the
//! SDK under `/sdk/`, which carries the stylesheet's typeface; images only
//! inline or from the bundle. The iframe
//! loads these without `allow-same-origin`, so the document has an opaque
//! origin and `'self'` would match nothing; the bundle path is spelled out
//! with the host the browser used.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::Wicket;
use crate::schema::safe_join;

pub fn routes() -> Router<Arc<Wicket>> {
    Router::new()
        .route("/plugins/{name}/{version}/{*path}", get(bundle))
        .route("/sdk/v1/{*path}", get(sdk))
}

async fn bundle(
    State(state): State<Arc<Wicket>>,
    Path((name, version, path)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let Ok(version) = version.parse::<u32>() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(plugin) = state.registry.fetch_version(&name, version) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let dir = plugin.path.clone();
    let Some(file) = safe_join(&dir, &path).filter(|f| f.is_file()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(body) = tokio::fs::read(&file).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("127.0.0.1");
    let origin = format!("http://{host}");
    let bundle = format!("{origin}/plugins/{name}/{version}/");
    let csp = [
        "default-src 'none'".to_string(),
        format!("script-src 'unsafe-inline' {bundle} {origin}/sdk/"),
        format!("style-src 'unsafe-inline' {bundle} {origin}/sdk/"),
        format!("img-src data: blob: {bundle} {origin}/sdk/"),
        format!("font-src data: {bundle} {origin}/sdk/"),
        format!("media-src data: blob: {bundle}"),
        "connect-src 'none'".to_string(),
        "form-action 'none'".to_string(),
        "base-uri 'none'".to_string(),
        format!(
            "frame-ancestors 'self' {}",
            super::shell_origins().join(" ")
        ),
    ]
    .join("; ");

    let mime = mime_guess::from_path(&file).first_or_octet_stream();
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime.as_ref()).unwrap(),
            ),
            (
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_str(&csp).unwrap(),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ),
            // A sandboxed view has an opaque origin, and CSS masks (the icon
            // set) and fonts load only from a server that says so.
            (
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            ),
        ],
        body,
    )
        .into_response()
}

async fn sdk(State(state): State<Arc<Wicket>>, Path(path): Path<String>) -> Response {
    let Some(dir) = &state.config().sdk_dir else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(file) = safe_join(dir, &path).filter(|f| f.is_file()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(body) = tokio::fs::read(&file).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = mime_guess::from_path(&file).first_or_octet_stream();
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime.as_ref()).unwrap(),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ),
            // A sandboxed view has an opaque origin, and CSS masks (the icon
            // set) and fonts load only from a server that says so.
            (
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            ),
        ],
        body,
    )
        .into_response()
}
