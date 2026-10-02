//! Plugin bundles and the SDK.
//!
//! A view is served from the plugin's own folder, the store entry or the
//! linked directory under `/plugins/`, or from a stored bundle under
//! `/bundles/<hash>/view/`, with the CSP that makes the sandbox real: no
//! network at all.
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

use super::ApiState;
use crate::Pinrail;
use crate::schema::safe_join;
use sha2::{Digest, Sha256};

pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/plugins/{name}/{version}/{*path}", get(bundle))
        .route("/bundles/{hash}/view/{*path}", get(stored_bundle))
        .route("/sdk/v1/{*path}", get(sdk))
        .route("/preview/reviews/{id}", get(preview))
}

/// A review as the app shows it, in a browser: the plugin's view in a frame,
/// with the hand-over. The page reads the review's id from its address and
/// everything else from the API, so any id gets the same page.
async fn preview() -> Response {
    const PAGE: &str = include_str!("preview.html");
    let csp = "default-src 'self'; script-src 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:; frame-src 'self'; connect-src 'self'; form-action 'none'; base-uri 'none'; frame-ancestors 'none'";
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            ),
            (
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static(csp),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
        ],
        PAGE,
    )
        .into_response()
}

async fn bundle(
    State(state): State<Arc<Pinrail>>,
    Path((name, version, path)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let Ok(version) = version.parse::<u32>() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // Only what an installed copy holds, whichever way the plugin was
    // installed: a linked plugin is served from its developer's folder,
    // with its hidden files, dependencies and sources beside the view.
    if !pinrail_format::bundle::holds(&path) {
        return StatusCode::NOT_FOUND.into_response();
    }
    if is_fetch(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Ok(plugin) = state.plugins().fetch_version(&name, version) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let dir = plugin.path.clone();
    let Some(file) = safe_join(&dir, &path).filter(|f| f.is_file()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(body) = tokio::fs::read(&file).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let origin = origin(&headers);
    let base = format!("{origin}/plugins/{name}/{version}/");
    let mime = mime_guess::from_path(&file).first_or_octet_stream();
    let mut response = (view_headers(&origin, &base, mime.as_ref()), body).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

/// A file of a stored bundle's view. The address names the bundle, which
/// never changes, so the file may be cached for ever; a file that no longer
/// matches the bundle's listing is not that bundle's, and is not served.
async fn stored_bundle(
    State(state): State<Arc<Pinrail>>,
    Path((hash, path)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if is_fetch(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Ok(path) = pinrail_format::bundle::normalized(&format!("view/{path}")) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(Some(listing)) = state.bundles().listing(&hash) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(file) = listing.file(&path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let etag = format!("\"{}\"", file.sha256);
    let caching = [
        (
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ),
        (header::ETAG, HeaderValue::from_str(&etag).unwrap()),
    ];
    if headers
        .get(header::IF_NONE_MATCH)
        .is_some_and(|sent| sent.as_bytes() == etag.as_bytes())
    {
        return (StatusCode::NOT_MODIFIED, caching).into_response();
    }
    let read = tokio::fs::read(state.bundles().path(&hash).join(&path)).await;
    let body = match read {
        Ok(body) if format!("{:x}", Sha256::digest(&body)) == file.sha256 => body,
        _ => {
            eprintln!("pinrail: bundle {hash}: {path} no longer matches its listing");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let origin = origin(&headers);
    let base = format!("{origin}/bundles/{hash}/view/");
    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    (caching, view_headers(&origin, &base, mime.as_ref()), body).into_response()
}

/// A view loads its files as a page, scripts, styles, fonts and images,
/// never with fetch, which its policy forbids: a fetch comes from another
/// page, reading what is not its own.
fn is_fetch(headers: &HeaderMap) -> bool {
    headers
        .get("sec-fetch-dest")
        .is_some_and(|dest| dest.as_bytes() == b"empty")
}

/// The app's address as the browser used it.
fn origin(headers: &HeaderMap) -> String {
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("127.0.0.1");
    format!("http://{host}")
}

/// The headers of a file of a view whose files are under `base`.
fn view_headers(origin: &str, base: &str, mime: &str) -> [(header::HeaderName, HeaderValue); 4] {
    let csp = [
        // The frame's sandbox attribute does nothing for a file loaded as a
        // page of its own; this gives it the same opaque origin either way,
        // so it can never act as the API's origin
        "sandbox allow-scripts".to_string(),
        "default-src 'none'".to_string(),
        format!("script-src 'unsafe-inline' {base} {origin}/sdk/"),
        format!("style-src 'unsafe-inline' {base} {origin}/sdk/"),
        format!("img-src data: blob: {base} {origin}/sdk/"),
        format!("font-src data: {base} {origin}/sdk/"),
        format!("media-src data: blob: {base}"),
        "connect-src 'none'".to_string(),
        "form-action 'none'".to_string(),
        "base-uri 'none'".to_string(),
        format!(
            "frame-ancestors 'self' {}",
            super::shell_origins().join(" ")
        ),
    ]
    .join("; ");
    [
        (header::CONTENT_TYPE, HeaderValue::from_str(mime).unwrap()),
        // Views load these as scripts, styles and fonts, which ignore a
        // policy; opened as a page, one gets an opaque origin and runs
        // nothing
        (
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_str(&csp).unwrap(),
        ),
        (
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ),
        // A sandboxed view has an opaque origin, which it sends as `null`,
        // and CSS masks (the icon set) and fonts load only from a server
        // that allows it; no other website may read these files.
        (
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("null"),
        ),
    ]
}

async fn sdk(State(state): State<Arc<Pinrail>>, Path(path): Path<String>) -> Response {
    let Some(dir) = &state.config().sdk_dir else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // a developer may point the app at a working tree: its hidden files
    // are not the SDK's to serve
    if path.split('/').any(|part| part.starts_with('.')) {
        return StatusCode::NOT_FOUND.into_response();
    }
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
            // a script, a stylesheet or a font loaded by a view ignores this;
            // a file opened as a page runs with no origin and no rights
            (
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static("sandbox; default-src 'none'"),
            ),
        ],
        body,
    )
        .into_response()
}
