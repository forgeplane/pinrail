//! The HTTP API: JSON under `/api/v1`, plugin bundles under `/plugins`, the
//! SDK under `/sdk/v1`, all bound to loopback.

mod artifacts;
mod error;
mod files;
mod guard;
mod plugins;
mod reviews;
mod settings;
mod sse;

use std::sync::Arc;
use std::time::Duration;

use axum::{Json, Router, extract::FromRef, extract::State, routing::get};
use chrono::{DateTime, Utc};
use serde::Serialize;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::error::Error;
use crate::{Pinrail, server_info};

#[derive(Debug, Serialize)]
pub struct Info {
    pub version: &'static str,
    pub data_dir: String,
    pub port: u16,
    pub pid: u32,
    pub started_at: DateTime<Utc>,
    pub user: String,
    /// The files stored beside reviews: how many, and their bytes.
    pub artifacts: ArtifactTotals,
}

#[derive(Debug, Serialize)]
pub struct ArtifactTotals {
    pub count: u64,
    pub bytes: u64,
}

/// What a handler can reach: the application, and what belongs to this run
/// of the server rather than to the application itself.
#[derive(Clone)]
pub struct ApiState {
    app: Arc<Pinrail>,
    /// When this server started, for `/info` and for `server.json`.
    started_at: DateTime<Utc>,
}

impl ApiState {
    fn new(app: Arc<Pinrail>) -> Self {
        ApiState {
            app,
            started_at: Utc::now(),
        }
    }
}

// Handlers ask for the application and get it out of the server's state.
impl FromRef<ApiState> for Arc<Pinrail> {
    fn from_ref(state: &ApiState) -> Arc<Pinrail> {
        state.app.clone()
    }
}

pub fn router(app: Arc<Pinrail>) -> Router {
    router_with(ApiState::new(app))
}

fn router_with(state: ApiState) -> Router {
    Router::new()
        .route("/api/v1/info", get(info))
        .merge(reviews::routes())
        .merge(artifacts::routes())
        .merge(plugins::routes())
        .merge(sse::routes())
        .merge(files::routes())
        .merge(settings::routes())
        .layer(axum::extract::DefaultBodyLimit::max(guard::JSON_LIMIT))
        .layer(axum::middleware::from_fn(guard::body_limit))
        // inside CORS, so a preflight is answered before a write is judged
        .layer(axum::middleware::from_fn(guard::json_writes))
        .layer(cors())
        // outermost: a request from a page that rebound its name to loopback
        // is refused before anything else looks at it
        .layer(axum::middleware::from_fn(guard::loopback_host))
        .with_state(state)
}

async fn info(State(state): State<ApiState>) -> Json<Info> {
    let config = state.app.config();
    let (count, bytes) = state.app.artifacts().totals().unwrap_or((0, 0));
    Json(Info {
        version: crate::VERSION,
        data_dir: config.data_dir.display().to_string(),
        port: config.port,
        pid: std::process::id(),
        started_at: state.started_at,
        user: config.user.clone(),
        artifacts: ArtifactTotals { count, bytes },
    })
}

/// Serves the API until `shutdown` resolves. Advertises itself in
/// `server.json` while it runs; every 30 seconds it sweeps expired reviews,
/// the reviews past the days the history keeps (when it keeps a limited
/// number), and blobs no review names that are more than an hour old.
pub async fn serve(
    app: Arc<Pinrail>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let state = ApiState::new(app);
    let listener = tokio::net::TcpListener::bind(state.app.config().bind_addr()).await?;
    server_info::write(state.app.config(), state.started_at)?;

    let sweeper = {
        let app = state.app.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(30));
            loop {
                tick.tick().await;
                if let Err(error) = app.reviews().sweep_expired() {
                    eprintln!("pinrail: expiry sweep failed: {error}");
                }
                let keep_days = app
                    .settings()
                    .value("/history/keep_days")
                    .as_u64()
                    .map(|d| d as u32);
                if let Err(error) = app.reviews().sweep_history(keep_days) {
                    eprintln!("pinrail: history sweep failed: {error}");
                }
                // after the history: what a swept review carried is free to go
                let hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);
                if let Err(error) = app.artifacts().sweep(hour_ago) {
                    eprintln!("pinrail: artifacts sweep failed: {error}");
                }
            }
        })
    };
    // an edit to settings.json outside the app is noticed within a second
    let watcher = {
        let app = state.app.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                tick.tick().await;
                if let Err(error) = app.settings().reload() {
                    eprintln!("pinrail: settings change not announced: {error}");
                }
            }
        })
    };

    let result = axum::serve(listener, router_with(state.clone()))
        .with_graceful_shutdown(shutdown)
        .await;
    sweeper.abort();
    watcher.abort();
    server_info::remove(state.app.config());
    result
}

/// The origins the shell runs on: the desktop app's own origin, plus, in
/// debug builds, the Vite dev server and whatever `PINRAIL_SHELL_ORIGIN`
/// names (the shell's tests run it elsewhere). The API answers
/// cross-origin requests from these only, and plugin bundles let only
/// these frame them. Nothing else can read the API or embed a view.
pub(crate) fn shell_origins() -> Vec<String> {
    let mut origins = vec![
        "tauri://localhost".to_string(),
        "http://tauri.localhost".to_string(),
    ];
    if cfg!(debug_assertions) {
        origins.push("http://localhost:5173".to_string());
        if let Ok(extra) = std::env::var("PINRAIL_SHELL_ORIGIN")
            && !extra.is_empty()
        {
            origins.push(extra);
        }
    }
    origins
}

fn cors() -> CorsLayer {
    let origins = shell_origins()
        .into_iter()
        .map(|origin| origin.parse().unwrap())
        .collect::<Vec<_>>();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PATCH,
            axum::http::Method::DELETE,
        ])
        .allow_headers([axum::http::header::CONTENT_TYPE])
}

/// Parses an optional JSON body: empty means an empty object.
pub(crate) fn parse_body(bytes: &[u8]) -> Result<serde_json::Value, Error> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(serde_json::Value::Object(Default::default()));
    }
    serde_json::from_slice(bytes).map_err(|_| Error::invalid("", "body is not valid JSON"))
}
