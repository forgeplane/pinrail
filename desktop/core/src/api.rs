//! The HTTP API. Everything is JSON under `/api/v1`, bound to loopback.

use std::sync::Arc;

use axum::{Json, Router, extract::State, routing::get};
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::Config;

/// What every handler can reach.
#[derive(Debug)]
pub struct AppState {
    pub config: Config,
    pub started_at: DateTime<Utc>,
}

impl AppState {
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new(Self {
            config,
            started_at: Utc::now(),
        })
    }
}

#[derive(Debug, Serialize)]
pub struct Info {
    pub version: &'static str,
    pub data_dir: String,
    pub port: u16,
    pub pid: u32,
    pub started_at: DateTime<Utc>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/info", get(info))
        .with_state(state)
}

async fn info(State(state): State<Arc<AppState>>) -> Json<Info> {
    Json(Info {
        version: crate::VERSION,
        data_dir: state.config.data_dir.display().to_string(),
        port: state.config.port,
        pid: std::process::id(),
        started_at: state.started_at,
    })
}

/// Serves the API on the configured loopback address until `shutdown` resolves.
pub async fn serve(
    state: Arc<AppState>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(state.config.bind_addr()).await?;
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn info_reports_the_server() {
        let state = AppState::new(Config {
            data_dir: "/tmp/w".into(),
            port: 4747,
        });
        let app = router(state);
        let response = app
            .oneshot(Request::get("/api/v1/info").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let info: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(info["version"], crate::VERSION);
        assert_eq!(info["data_dir"], "/tmp/w");
        assert_eq!(info["port"], 4747);
        assert_eq!(info["pid"], std::process::id());
        assert!(info["started_at"].is_string());
    }
}
