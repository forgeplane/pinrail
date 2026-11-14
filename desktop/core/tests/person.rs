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

/// The writes only the person makes, through the app's window: they need
/// the person token from whoever sends them. Every other write is open to
/// programs, which act as themselves.
const PERSON_ONLY: [(&str, &str); 21] = [
    ("POST", "/api/v1/sessions/{agent}/{id}/messages"),
    ("DELETE", "/api/v1/sessions/{agent}/{id}/messages/{message}"),
    ("PUT", "/api/v1/sessions/{agent}/{id}/archive"),
    ("DELETE", "/api/v1/sessions/{agent}/{id}/archive"),
    ("PUT", "/api/v1/sessions/{agent}/{id}/seen"),
    ("DELETE", "/api/v1/sessions/{agent}/{id}/seen"),
    ("PATCH", "/api/v1/settings"),
    ("POST", "/api/v1/projects"),
    ("PATCH", "/api/v1/projects/{id}"),
    ("DELETE", "/api/v1/projects/{id}"),
    ("PUT", "/api/v1/projects/{id}/image"),
    ("DELETE", "/api/v1/projects/{id}/image"),
    ("PUT", "/api/v1/repos/{repo}/project"),
    ("PATCH", "/api/v1/repos/{repo}"),
    ("DELETE", "/api/v1/repos/{repo}"),
    ("POST", "/api/v1/themes"),
    ("PUT", "/api/v1/reviews/{id}/star"),
    ("DELETE", "/api/v1/reviews/{id}/star"),
    ("POST", "/api/v1/reviews/{id}/viewed"),
    ("POST", "/api/v1/runs"),
    ("POST", "/api/v1/runs/{run}/stop"),
];

/// Every write route the API declares, as its method and path, read from
/// the router's source, so that a route added later is checked too.
fn write_routes() -> Vec<(String, String)> {
    let mut routes = Vec::new();
    let folder = concat!(env!("CARGO_MANIFEST_DIR"), "/src/api");
    for entry in std::fs::read_dir(folder).unwrap() {
        let source = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        let mut rest = source.as_str();
        while let Some(at) = rest.find(".route(") {
            // the call, up to its closing parenthesis
            let call = &rest[at + ".route(".len()..];
            let mut depth = 1;
            let end = call
                .char_indices()
                .find(|(_, c)| {
                    match c {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {}
                    }
                    depth == 0
                })
                .map(|(i, _)| i)
                .unwrap();
            let call = &call[..end];
            let path = call.split('"').nth(1).unwrap().to_string();
            for method in ["post", "put", "patch", "delete"] {
                let named = call
                    .match_indices(&format!("{method}("))
                    .any(|(i, _)| !call[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_'));
                if named {
                    routes.push((method.to_uppercase(), path.clone()));
                }
            }
            rest = &rest[at + ".route(".len() + end..];
        }
    }
    routes.sort();
    routes
}

/// A path the route answers, with a value in each of its parameters.
fn concrete(path: &str) -> String {
    path.replace("{sha256}", &"0".repeat(64))
        .replace("{agent}", "claude-code")
        .replace("{repo}", "acme%2Fweb")
        .replace("{message}", "1")
        .split('/')
        .map(|part| match part.starts_with('{') {
            true => "x-1",
            false => part,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[tokio::test]
async fn every_write_needs_the_person_token_to_act_as_the_person() {
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
    let routes = write_routes();
    assert!(routes.len() > 40, "{routes:?}");
    // the list names routes that exist
    for (method, path) in PERSON_ONLY {
        assert!(
            routes.contains(&(method.to_string(), path.to_string())),
            "{method} {path} is no route"
        );
    }
    for (method, path) in &routes {
        let person_only = PERSON_ONLY.contains(&(method.as_str(), path.as_str()));
        let at = concrete(path);
        // saying it is the window is never enough
        for headers in [&forged[..], &wrong[..]] {
            let (status, body) = send(&app, method, &at, headers).await;
            assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "{method} {path} {headers:?}"
            );
            assert_eq!(body["error"], "person_token", "{method} {path}");
        }
        let (status, _) = send(&app, method, &at, &window).await;
        assert_ne!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {path} from the window"
        );
        // a program that acts as itself is refused only what the person does
        let (status, _) = send(&app, method, &at, &[("x-pinrail-client", "cli")]).await;
        assert_eq!(
            status == StatusCode::UNAUTHORIZED,
            person_only,
            "{method} {path} from a program"
        );
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
