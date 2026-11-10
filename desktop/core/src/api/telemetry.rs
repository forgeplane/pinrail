//! `/api/v1/telemetry`: where usage data stands, the events that wait to
//! be sent and the last ones sent, and the features the app's window
//! reports. The switches are settings, changed with `/api/v1/settings`.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::Value;

use super::ApiState;
use super::error::ApiError;
use super::parse_body;
use crate::Pinrail;
use crate::error::Error;
use crate::telemetry::{self, Event, Name};

/// The headers the CLI names its command and its agent in, on every
/// request it makes.
const COMMAND_HEADER: &str = "x-pinrail-command";
const AGENT_HEADER: &str = "x-pinrail-agent";

pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/telemetry", get(show))
        .route("/api/v1/telemetry/features", post(feature))
        .route("/api/v1/telemetry/crashes", post(crash))
}

async fn show(State(state): State<Arc<Pinrail>>) -> Json<Value> {
    Json(state.telemetry().status())
}

/// Counts a feature the window reports: `{"feature": "command_palette"}`,
/// one of [`telemetry::WINDOW_FEATURES`].
async fn feature(State(state): State<Arc<Pinrail>>, body: Bytes) -> Response {
    let counted = parse_body(&body).and_then(|body: Value| {
        let name = body["feature"].as_str().unwrap_or_default();
        let feature = telemetry::window_feature(name)
            .ok_or_else(|| Error::invalid("/feature", "is not a feature the app counts"))?;
        state.telemetry().record(Event::FeatureUsed { feature });
        Ok(())
    });
    match counted {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => ApiError(error).into_response(),
    }
}

/// Keeps an uncaught error of the window as a crash report, scrubbed, to
/// send at the next start: `{"message", "stack"}`. Answers 204 whether it
/// was kept or crash reports are off.
async fn crash(State(state): State<Arc<Pinrail>>, body: Bytes) -> Response {
    match parse_body(&body) {
        Ok(body) => {
            let text = |key: &str| body[key].as_str().unwrap_or_default().to_string();
            let report = telemetry::CrashReport::new(
                telemetry::CrashKind::Ui,
                &text("message"),
                &text("stack"),
            );
            state.telemetry().keep_crash(&report);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(error) => ApiError(error).into_response(),
    }
}

/// Counts a request of the CLI by the command and the agent it names.
pub async fn count_cli(State(state): State<ApiState>, request: Request, next: Next) -> Response {
    if let Some(event) = cli_event(request.headers()) {
        state.app.telemetry().record(event);
    }
    next.run(request).await
}

fn cli_event(headers: &HeaderMap) -> Option<Event> {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    Some(Event::CliUsed {
        command: telemetry::cli_command(header(COMMAND_HEADER)?),
        agent: Name::of(header(AGENT_HEADER).unwrap_or("none")),
    })
}
