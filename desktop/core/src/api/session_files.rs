//! `/api/v1/sessions/{agent}/{id}/files/{name}`: a file the person attaches to a
//! session's prompt, such as a pasted screenshot, saved where the agent can
//! open it by its path. Only the person saves one.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::put;
use futures_util::StreamExt;
use serde_json::json;

use super::ApiState;
use super::error::ApiError;
use super::guard::{Person, megabytes, refuse};
use crate::Pinrail;
use crate::error::Error;
use crate::session_files::MAX_BYTES;

pub fn routes() -> Router<ApiState> {
    Router::new().route("/api/v1/sessions/{agent}/{id}/files/{name}", put(save))
}

/// Saves the body, sent as `application/octet-stream`, under `name`, or
/// that name with a number when it is taken. 201 with
/// `{path, name}`: the file's absolute path and the name it was saved
/// under; 404 when the session is unknown, 413 past the cap.
async fn save(
    _person: Person,
    State(state): State<Arc<Pinrail>>,
    Path((agent, id, name)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    let octets = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/octet-stream"));
    if !octets {
        return Ok(refuse(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "a file must be sent as Content-Type: application/octet-stream".into(),
        ));
    }
    let too_large = || {
        refuse(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too_large",
            format!("a file may be {} at most", megabytes(MAX_BYTES as usize)),
        )
    };
    let declared = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    if declared.is_some_and(|n| n > MAX_BYTES) {
        return Ok(too_large());
    }
    let mut saving = state.session_files().begin(&agent, &id, &name)?;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::invalid("", format!("the upload broke off: {e}")))?;
        if !saving.write(&chunk)? {
            return Ok(too_large());
        }
    }
    let path = saving.finish()?;
    let saved = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok((
        StatusCode::CREATED,
        axum::Json(json!({ "path": path, "name": saved })),
    )
        .into_response())
}
