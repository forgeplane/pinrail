//! What the tests that drive the API share: an app on a fresh data
//! directory, and a request through its router.

// each test file uses its own share of these
#![allow(dead_code)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use pinrail_core::api::router;
use pinrail_core::{Config, Pinrail};
use serde_json::Value;
use tower::ServiceExt;

/// A core on a data directory of its own, and its router.
pub struct App {
    /// the data directory, removed when the test ends
    pub dir: tempfile::TempDir,
    pub state: Arc<Pinrail>,
    pub router: Router,
}

/// An app with `list` and `feedback` installed from its catalog, as the
/// person would choose them.
pub fn app() -> App {
    app_with(|_| {})
}

/// An app with nothing installed, as it first opens.
pub fn app_without_plugins() -> App {
    open(|_| {})
}

/// An app whose configuration the test adjusts first, with `list` and
/// `feedback` installed.
pub fn app_with(adjust: impl FnOnce(&mut Config)) -> App {
    let app = open(adjust);
    install(&app, &["forgeplane/list", "forgeplane/feedback"]);
    app
}

/// Installs official plugins from the app's catalog. The install is
/// awaited on a runtime of its own, so a test's runtime can call this.
pub fn install(app: &App, ids: &[&str]) {
    let state = app.state.clone();
    let ids: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for id in ids {
                state.plugins().install_offered(&id).await.unwrap();
            }
        })
    })
    .join()
    .unwrap();
}

fn open(adjust: impl FnOnce(&mut Config)) -> App {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::new(dir.path(), 0);
    config.user = "tester".into();
    adjust(&mut config);
    let state = Arc::new(Pinrail::open(config).unwrap());
    App {
        router: router(state.clone()),
        state,
        dir,
    }
}

/// Inspects or seeds the database directly; the app does not expose its
/// connection to callers.
pub fn db(app: &App) -> pinrail_core::db::Db {
    pinrail_core::db::Db::open(&app.state.config().db_path()).unwrap()
}

/// A request as a local client sends it, answered as JSON, or the text when
/// the answer is not JSON.
pub async fn call(app: &App, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:4747")
        .header("content-type", "application/json")
        .body(
            body.map(|b| Body::from(b.to_string()))
                .unwrap_or_else(Body::empty),
        )
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    (status, value)
}
