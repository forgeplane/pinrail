//! The HTTP API: JSON under `/api/v1`, plugin bundles under `/plugins`, the
//! SDK under `/sdk/v1`, all bound to loopback.

mod files;
mod plugins;
mod reviews;
mod settings;
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
    pub settings: Arc<crate::settings::Store>,
    pub db: Arc<Db>,
    pub registry: Arc<Registry>,
    pub reviews: Reviews,
    /// plugin installs under way or done, by job id
    pub jobs: Arc<crate::install::Jobs>,
}

impl AppState {
    /// Opens the database, writes out the built-in plugin, scans the plugin
    /// directories and wires the service together.
    /// Applies a partial change to the settings and announces what changed,
    /// for the API and for the app itself (the tray's pause, for one).
    pub fn change_settings(&self, patch: &serde_json::Value) -> Result<serde_json::Value, Error> {
        // a plugin's own settings are the plugin's schema to judge; a name
        // that is not registered now is kept as it is
        let mut violations = Vec::new();
        if let Some(plugins) = patch.get("plugins").and_then(serde_json::Value::as_object) {
            for (name, change) in plugins {
                if let Some(plugin) = self.registry.get(name) {
                    violations.extend(plugin.validate_settings(change));
                }
            }
        }
        if !violations.is_empty() {
            violations.sort_by(|a, b| a.path.cmp(&b.path));
            return Err(Error::Invalid(violations));
        }
        let (after, keys) = self.settings.patch(patch)?;
        if !keys.is_empty() {
            settings::announce(self, &keys)?;
        }
        Ok(after)
    }

    pub fn open(config: Config) -> Result<Arc<Self>, Error> {
        std::fs::create_dir_all(&config.data_dir)?;
        let settings = Arc::new(crate::settings::Store::open(&config.data_dir));
        let db = Arc::new(Db::open(&config.db_path())?);
        let builtin = plugin_store::install_builtin(&config.builtin_plugins_dir())?;
        let user = config.user_plugins_dir();
        let _ = std::fs::create_dir_all(&user);
        let mut defaults = vec![builtin, user];
        defaults.extend(config.plugin_dirs.iter().cloned());
        let records = db.installed_plugins()?;
        let registry = Arc::new(
            Registry::open(
                defaults,
                records,
                config.plugin_store_dir(),
                config.snapshots_dir(),
            )
            .map_err(Error::Internal)?,
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
            settings,
            db,
            registry,
            reviews,
            jobs: Arc::new(crate::install::Jobs::default()),
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
        .merge(settings::routes())
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
    // an edit to settings.json outside the app is noticed within a second
    let watcher = {
        let state = state.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                tick.tick().await;
                if let Some(keys) = state.settings.reload_if_changed()
                    && let Err(error) = settings::announce(&state, &keys)
                {
                    eprintln!("wicket: settings change not announced: {error}");
                }
            }
        })
    };

    let result = axum::serve(listener, router(state.clone()))
        .with_graceful_shutdown(shutdown)
        .await;
    sweeper.abort();
    watcher.abort();
    server_info::remove(&state.config);
    result
}

/// The origins the shell runs on: the desktop app's own origin, plus, in
/// debug builds, the Vite dev server and whatever `WICKET_SHELL_ORIGIN`
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
        if let Ok(extra) = std::env::var("WICKET_SHELL_ORIGIN")
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
