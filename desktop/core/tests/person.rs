//! The person token: what only the person may do, and what a request that
//! says it comes from the app's window must prove.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::app;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

/// A request with the headers given, and the status it is answered with.
async fn send(
    app: &common::App,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:4747")
        .header("content-type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = app
        .router
        .clone()
        .oneshot(request.body(Body::from("{}")).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// What only the person does, through the app's window.
const PERSON_ONLY: [(&str, &str); 16] = [
    ("POST", "/api/v1/sessions/claude-code/s-1/messages"),
    ("DELETE", "/api/v1/sessions/claude-code/s-1/messages/1"),
    ("PUT", "/api/v1/sessions/claude-code/s-1/archive"),
    ("DELETE", "/api/v1/sessions/claude-code/s-1/archive"),
    ("PATCH", "/api/v1/settings"),
    ("POST", "/api/v1/projects"),
    ("PATCH", "/api/v1/projects/p-1"),
    ("DELETE", "/api/v1/projects/p-1"),
    ("PUT", "/api/v1/repos/acme%2Fweb/project"),
    ("PATCH", "/api/v1/repos/acme%2Fweb"),
    ("POST", "/api/v1/themes"),
    ("PUT", "/api/v1/reviews/r-1/star"),
    ("DELETE", "/api/v1/reviews/r-1/star"),
    ("POST", "/api/v1/reviews/r-1/viewed"),
    ("PUT", "/api/v1/sessions/claude-code/s-1/seen"),
    ("DELETE", "/api/v1/sessions/claude-code/s-1/seen"),
];

/// What the CLI does too, and the app's window does as the person.
const SHARED: [(&str, &str); 4] = [
    ("POST", "/api/v1/reviews/r-1/decision"),
    ("POST", "/api/v1/reviews/r-1/discard"),
    ("DELETE", "/api/v1/reviews/r-1"),
    ("PATCH", "/api/v1/items/PIN-1"),
];

#[tokio::test]
async fn only_the_person_s_token_lets_a_program_act_as_the_person() {
    let app = app();
    let token = app.state.config().person_token.clone();
    let bearer = format!("Bearer {token}");
    let window = [
        ("origin", "tauri://localhost"),
        ("authorization", bearer.as_str()),
    ];
    let forged = [("origin", "tauri://localhost")];
    let wrong = [
        ("origin", "tauri://localhost"),
        ("authorization", "Bearer nope"),
    ];

    for (method, path) in PERSON_ONLY {
        // a program on this machine, with or without the window's origin
        for headers in [&[][..], &forged[..], &wrong[..]] {
            let (status, body) = send(&app, method, path, headers).await;
            assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "{method} {path} {headers:?}"
            );
            assert_eq!(body["error"], "person_token", "{method} {path}");
        }
        let (status, _) = send(&app, method, path, &window).await;
        assert_ne!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }

    for (method, path) in SHARED {
        // saying it is the window is not enough
        for headers in [&forged[..], &wrong[..]] {
            let (status, _) = send(&app, method, path, headers).await;
            assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "{method} {path} {headers:?}"
            );
        }
        let (status, _) = send(&app, method, path, &window).await;
        assert_ne!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
        // the CLI acts as itself, and is recorded so
        let (status, _) = send(&app, method, path, &[("x-pinrail-client", "cli")]).await;
        assert_ne!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }

    // reading needs no token, from the window or anywhere
    for headers in [&[][..], &forged[..]] {
        let (status, _) = send(&app, "GET", "/api/v1/reviews", headers).await;
        assert_eq!(status, StatusCode::OK);
    }
}

#[tokio::test]
async fn each_core_has_a_token_of_its_own() {
    let (a, b) = (app(), app());
    let (a, b) = (
        &a.state.config().person_token,
        &b.state.config().person_token,
    );
    assert_eq!(a.len(), 64);
    assert_ne!(a, b);
}
