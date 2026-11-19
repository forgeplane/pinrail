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

/// An app as a person first starts it, with no settings written.
pub fn app_as_new() -> App {
    let app = open(|_| {});
    install(&app, &["forgeplane/list", "forgeplane/feedback"]);
    app
}

fn open(adjust: impl FnOnce(&mut Config)) -> App {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::new(dir.path(), 0);
    config.user = "tester".into();
    config.telemetry_env = false;
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

/// A request as the app's window sends it: from the app's origin, so a
/// decision or a discard made through it is recorded as the person's, and
/// with the person token. Answered as JSON, or the text when it is not JSON.
pub async fn call(app: &App, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:4747")
        .header("origin", "tauri://localhost")
        .header(
            "authorization",
            format!("Bearer {}", app.state.config().person_token),
        )
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

/// A request from a program on this machine, such as the CLI: no origin
/// and no person token, with these extra headers.
pub async fn call_as(
    app: &App,
    method: &str,
    path: &str,
    body: Option<Value>,
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
    let request = request
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

/// A terminal host served from a thread of the test, with its data in
/// `dir`, and its registration.
#[cfg(unix)]
pub fn host(dir: &std::path::Path) -> pinrail_host::registration::Registration {
    use std::time::{Duration, Instant};
    let data_dir = dir.join("host-data");
    let config = pinrail_host::server::Config {
        data_dir: data_dir.clone(),
        idle_exit: Duration::from_secs(600),
        ping_every: Duration::from_secs(15),
        // the programs are this process's own, through a holder on a thread
        holder: pinrail_host::holders::Start::Thread,
    };
    std::thread::spawn(move || pinrail_host::server::serve(config));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(registration) =
            pinrail_host::registration::Registration::read(&data_dir.join("host/host.json"))
        {
            return registration;
        }
        assert!(Instant::now() < deadline, "the host did not start");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A stand-in for Claude Code: it writes its arguments and its run token
/// into the folder it starts in, each file whole, then exits with the code in the folder's
/// `exit` file, or waits.
#[cfg(unix)]
pub const STAND_IN_AGENT: &str = r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo "1.0.0 (stand-in)"; exit 0; fi
echo "stand-in agent starting"
# each written whole, then renamed, so a test never reads half of one
printf '%s\n' "$@" > argv.part && mv argv.part argv
printf '%s' "$PINRAIL_RUN_TOKEN" > token.part && mv token.part token
if [ -f exit ]; then exit "$(cat exit)"; fi
# with a file named listen, it writes each line it is sent to heard
if [ -f listen ]; then while IFS= read -r line; do printf '%s\n' "$line" >> heard; done; exit 0; fi
exec sleep 600
"#;

/// Links the stand-in agent as `claude` into `dir/bin`, and returns the
/// `PATH` that finds it.
///
/// The script itself is written once, under Cargo's target folder, and
/// rewritten only when it changes: macOS scans a new executable for about
/// 25 seconds the first time it runs, which a fresh copy for each test
/// would pay every time.
#[cfg(unix)]
pub fn stand_in_path(dir: &std::path::Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    let script = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("stand-in-claude");
    if std::fs::read_to_string(&script).ok().as_deref() != Some(STAND_IN_AGENT) {
        // written beside it and renamed, as other tests may run it meanwhile
        let fresh = script.with_extension(format!("{}", std::process::id()));
        std::fs::write(&fresh, STAND_IN_AGENT).unwrap();
        std::fs::set_permissions(&fresh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::rename(&fresh, &script).unwrap();
    }
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(&script, bin.join("claude")).unwrap();
    format!("{}:/usr/bin:/bin", bin.display())
}
