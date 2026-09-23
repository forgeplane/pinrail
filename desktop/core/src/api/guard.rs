//! What keeps web pages out of a server that only listens on loopback.
//!
//! Listening on 127.0.0.1 keeps other machines out, not the browser on this
//! one. A page the person visits can rebind its own name to 127.0.0.1 and
//! then talk to the API as if it were same-origin: it reads every review
//! and can decide them. What it cannot forge is the `Host` header, which
//! still names the page's own site, so a request that names a host must
//! name a loopback one. Programs on the machine are trusted, and may leave
//! `Host` out; a browser always sends it.

use axum::Json;
use axum::extract::Request;
use axum::http::{StatusCode, header};
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
