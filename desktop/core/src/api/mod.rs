//! The HTTP API: JSON under `/api/v1`, plugin bundles under `/plugins`, the
//! SDK under `/sdk/v1`, all bound to loopback.

mod files;
mod plugins;
mod reviews;
mod sse;

use std::sync::Arc;
use std::time::Duration;

use axum::{Json, Router, extract::State, routing::get};
use chrono::{DateTime, Utc};
use serde::Serialize;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::db::Db;
use crate::error::Error;
use crate::events::Bus;
use crate::plugins::{self as plugin_store, Registry};
use crate::reviews::Reviews;
use crate::{Config, server_info};

/// What every handler can reach.
#[derive(Debug)]
pub struct AppState {
    pub config: Config,
    pub started_at: DateTime<Utc>,
    pub db: Arc<Db>,
    pub registry: Arc<Registry>,
    pub reviews: Reviews,
}

impl AppState {
    /// Opens the database, writes out the built-in plugin, scans the plugin
    /// directories and wires the service together.
    pub fn open(config: Config) -> Result<Arc<Self>, Error> {
        std::fs::create_dir_all(&config.data_dir)?;
        let db = Arc::new(Db::open(&config.db_path())?);
        let builtin = plugin_store::install_builtin(&config.builtin_plugins_dir())?;
        let user = config.user_plugins_dir();
        let _ = std::fs::create_dir_all(&user);
        let mut defaults = vec![builtin, user];
        defaults.extend(config.plugin_dirs.iter().cloned());
        let added = db.plugin_dirs()?.into_iter().map(Into::into).collect();
        let registry = Arc::new(
            Registry::open(defaults, added, config.snapshots_dir()).map_err(Error::Internal)?,
        );
        let reviews = Reviews::new(
            db.clone(),
            registry.clone(),
            Bus::new(),
            config.user.clone(),
        );
        Ok(Arc::new(Self {
            config,
            started_at: Utc::now(),
            db,
            registry,
            reviews,
        }))
    }
}

#[derive(Debug, Serialize)]
pub struct Info {
    pub version: &'static str,
    pub data_dir: String,
    pub port: u16,
    pub pid: u32,
    pub started_at: DateTime<Utc>,
    pub user: String,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/info", get(info))
        .merge(reviews::routes())
        .merge(plugins::routes())
        .merge(sse::routes())
        .merge(files::routes())
        .layer(cors())
        .with_state(state)
}

async fn info(State(state): State<Arc<AppState>>) -> Json<Info> {
    Json(Info {
        version: crate::VERSION,
        data_dir: state.config.data_dir.display().to_string(),
        port: state.config.port,
        pid: std::process::id(),
        started_at: state.started_at,
        user: state.config.user.clone(),
    })
}

/// Serves the API until `shutdown` resolves. Advertises itself in
/// `server.json` while it runs and sweeps expired reviews every 30 seconds.
pub async fn serve(
    state: Arc<AppState>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(state.config.bind_addr()).await?;
    server_info::write(&state.config, state.started_at)?;

    let sweeper = {
        let state = state.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(30));
            loop {
                tick.tick().await;
                if let Err(error) = state.reviews.sweep_expired() {
                    eprintln!("wicket: expiry sweep failed: {error}");
                }
            }
        })
    };

    let result = axum::serve(listener, router(state.clone()))
        .with_graceful_shutdown(shutdown)
        .await;
    sweeper.abort();
    server_info::remove(&state.config);
    result
}

/// The origins the shell runs on: the desktop app's own origin, plus the
/// Vite dev server in debug builds. The API answers cross-origin requests
/// from these only, and plugin bundles let only these frame them. Nothing
/// else can read the API or embed a view.
pub(crate) fn shell_origins() -> Vec<&'static str> {
    let mut origins = vec!["tauri://localhost", "http://tauri.localhost"];
    if cfg!(debug_assertions) {
        origins.push("http://localhost:5173");
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
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers([axum::http::header::CONTENT_TYPE])
}

/// Parses an optional JSON body: empty means an empty object.
pub(crate) fn parse_body(bytes: &[u8]) -> Result<serde_json::Value, Error> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(serde_json::Value::Object(Default::default()));
    }
    serde_json::from_slice(bytes).map_err(|_| Error::invalid("", "body is not valid JSON"))
}
