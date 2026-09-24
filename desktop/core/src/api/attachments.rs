//! `/api/v1/attachments/{sha256}`: whether a file is stored, and uploading
//! one. An upload is streamed to disk, hashed on the way, and refused the
//! moment it passes the cap; its body must say `application/octet-stream`,
//! which no web page can send across origins without a preflight.

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
use super::guard::{megabytes, refuse};
use crate::Pinrail;
use crate::attachments::UploadError;
use crate::error::Error;

pub fn routes() -> Router<ApiState> {
    Router::new().route("/api/v1/attachments/{sha256}", put(upload).head(stored))
}

/// 200 with the size as `Content-Length` when the blob is stored, 404 when not.
async fn stored(
    State(state): State<Arc<Pinrail>>,
    Path(sha256): Path<String>,
) -> Result<Response, ApiError> {
    Ok(match state.attachments().stored(&sha256)? {
        Some(blob) => (
            StatusCode::OK,
            [(header::CONTENT_LENGTH, blob.size.to_string())],
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

/// 201 with `{sha256, size}` once stored, 200 if it already was; 413 past
/// the cap, 422 when the bytes hash to something else.
async fn upload(
    State(state): State<Arc<Pinrail>>,
    Path(sha256): Path<String>,
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
            "an upload must send Content-Type: application/octet-stream".into(),
        ));
    }
    let attachments = state.attachments();
    if let Some(blob) = attachments.stored(&sha256)? {
        return Ok((
            StatusCode::OK,
            axum::Json(json!({ "sha256": blob.sha256, "size": blob.size })),
        )
            .into_response());
    }
    let too_large = |limit: u64| {
        refuse(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too_large",
            format!("an attachment may be {} at most", megabytes(limit as usize)),
        )
    };
    let declared = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    if declared.is_some_and(|n| n > attachments.max_bytes()) {
        return Ok(too_large(attachments.max_bytes()));
    }
    let mut upload = attachments.begin(&sha256)?;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::invalid("", format!("the upload broke off: {e}")))?;
        match upload.write(&chunk) {
            Ok(()) => {}
            Err(UploadError::TooLarge { limit }) => return Ok(too_large(limit)),
            Err(other) => return Err(upload_error(other)),
        }
    }
    match upload.finish() {
        Ok(blob) => Ok((
            StatusCode::CREATED,
            axum::Json(json!({ "sha256": blob.sha256, "size": blob.size })),
        )
            .into_response()),
        Err(UploadError::TooLarge { limit }) => Ok(too_large(limit)),
        Err(other) => Err(upload_error(other)),
    }
}

fn upload_error(error: UploadError) -> ApiError {
    match error {
        UploadError::Mismatch { sent, actual } => ApiError(Error::invalid(
            "/sha256",
            format!("the bytes hash to {actual}, not {sent}"),
        )),
        UploadError::Failed(error) => ApiError(error),
        UploadError::TooLarge { .. } => unreachable!("answered where it happens"),
    }
}
