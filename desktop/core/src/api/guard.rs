//! What keeps web pages out of a server that only listens on loopback.
//!
//! Listening on 127.0.0.1 keeps other machines out, not the browser on this
//! one. A page the person visits can rebind its own name to 127.0.0.1 and
//! then talk to the API as if it were same-origin: it reads every review
//! and can decide them. What it cannot forge is the `Host` header, which
//! still names the page's own site, so a request that names a host must
//! name a loopback one. Programs on the machine are trusted, and may leave
//! `Host` out; a browser always sends it.
//!
//! A page on any site can also send a "simple" request cross-origin without
//! asking first: a POST whose body is `text/plain`, a form, or nothing at
//! all. CORS then only hides the answer; the server has already acted. So
//! a write must say it is JSON, which no page can send cross-origin
//! without a preflight, and the preflight is refused to every origin but
//! the shell's.

use axum::Json;
use axum::extract::Request;
use axum::http::{Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// The names the server answers to, without the port.
const LOOPBACK: [&str; 3] = ["127.0.0.1", "localhost", "[::1]"];

/// Refuses a request whose `Host` names anything but loopback.
pub(crate) async fn loopback_host(request: Request, next: Next) -> Response {
    match request.headers().get(header::HOST) {
        None => next.run(request).await,
        Some(host) if host.to_str().is_ok_and(is_loopback) => next.run(request).await,
        Some(host) => refuse(
            StatusCode::FORBIDDEN,
            "forbidden_host",
            format!(
                "this server answers to 127.0.0.1, localhost and [::1] only, not {}",
                String::from_utf8_lossy(host.as_bytes())
            ),
        ),
    }
}

/// Refuses a POST or PATCH that does not say its body is JSON, empty
/// bodies included: `withdraw` and `discard` take none, and act.
pub(crate) async fn json_writes(request: Request, next: Next) -> Response {
    let writes = request.method() == Method::POST || request.method() == Method::PATCH;
    let json = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(is_json);
    if !writes || json {
        return next.run(request).await;
    }
    refuse(
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "unsupported_media_type",
        "a POST or PATCH must send Content-Type: application/json, even with no body".into(),
    )
}

/// `application/json`, with or without parameters such as a charset.
fn is_json(value: &str) -> bool {
    value
        .split(';')
        .next()
        .is_some_and(|essence| essence.trim().eq_ignore_ascii_case("application/json"))
}

/// `host` or `host:port`, with the host one of [`LOOPBACK`].
fn is_loopback(value: &str) -> bool {
    let name = match value.rsplit_once(':') {
        // `[::1]` alone has colons inside the brackets and no port
        Some((name, port)) if !name.is_empty() && !port.contains(']') => name,
        _ => value,
    };
    LOOPBACK.iter().any(|l| name.eq_ignore_ascii_case(l))
}

/// A refusal in the shape every API error has.
pub(crate) fn refuse(status: StatusCode, kind: &str, message: String) -> Response {
    (
        status,
        Json(json!({ "error": kind, "message": message, "violations": [] })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_with_or_without_parameters() {
        for ok in [
            "application/json",
            "application/json; charset=utf-8",
            "Application/JSON",
        ] {
            assert!(is_json(ok), "{ok}");
        }
        for bad in [
            "text/plain",
            "application/x-www-form-urlencoded",
            "multipart/form-data; boundary=x",
            "application/jsonp",
            "",
        ] {
            assert!(!is_json(bad), "{bad}");
        }
    }

    #[test]
    fn loopback_names_with_or_without_a_port() {
        for ok in [
            "127.0.0.1",
            "127.0.0.1:4747",
            "localhost:4747",
            "LOCALHOST",
            "[::1]",
            "[::1]:4747",
        ] {
            assert!(is_loopback(ok), "{ok}");
        }
        for bad in [
            "evil.example",
            "evil.example:4747",
            "127.0.0.1.evil.example:4747",
            "localhost.evil.example",
            "0.0.0.0:4747",
            "[::2]:4747",
            "",
        ] {
            assert!(!is_loopback(bad), "{bad}");
        }
    }
}
