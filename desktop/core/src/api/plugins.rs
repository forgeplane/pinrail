//! `/api/v1/plugins`: registered plugins, snapshot versions, reload, and
//! adding a plugin directory.

use std::path::Path as FsPath;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use super::{AppState, parse_body};
use crate::error::Error;
use crate::events;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/plugins", get(index))
        .route("/api/v1/plugins/reload", post(reload))
        .route("/api/v1/plugins/dirs", post(add_dir))
        .route("/api/v1/plugins/{name}/versions", get(versions))
}

async fn index(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "dirs": state.registry.dirs().iter().map(|d| d.display().to_string()).collect::<Vec<_>>(),
        "plugins": state.registry.all().iter().map(|p| p.to_json()).collect::<Vec<_>>(),
    }))
}

async fn versions(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, Error> {
    let current = state.registry.get(&name);
    let mut versions: Vec<u32> = Vec::new();
    let dir = state.config.snapshots_dir().join(&name);
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Ok(v) = entry.file_name().to_string_lossy().parse::<u32>()
                && entry.path().join("manifest.json").is_file()
            {
                versions.push(v);
            }
        }
    }
    if let Some(p) = &current
        && p.usable()
        && !versions.contains(&p.version)
    {
        versions.push(p.version);
    }
    if current.is_none() && versions.is_empty() {
        return Err(Error::NotFound(name));
    }
    versions.sort_unstable();
    Ok(Json(json!({
        "name": name,
        "current": current.as_ref().filter(|p| p.usable()).map(|p| p.version),
        "versions": versions,
    })))
}

async fn reload(State(state): State<Arc<AppState>>) -> Result<Json<Value>, Error> {
    let count = state.registry.reload().map_err(Error::Internal)?;
    announce(&state)?;
    Ok(Json(json!({ "ok": true, "count": count })))
}

async fn add_dir(State(state): State<Arc<AppState>>, body: Bytes) -> Result<Json<Value>, Error> {
    let body = parse_body(&body)?;
    let Some(dir) = body.get("dir").and_then(Value::as_str) else {
        return Err(Error::invalid("/dir", "is required"));
    };
    let count = state
        .registry
        .add_dir(FsPath::new(dir))
        .map_err(|message| Error::invalid("/dir", message))?;
    for added in state.registry.added_dirs() {
        state.db.add_plugin_dir(&added.display().to_string())?;
    }
    announce(&state)?;
    Ok(Json(json!({
        "ok": true,
        "count": count,
        "dirs": state.registry.dirs().iter().map(|d| d.display().to_string()).collect::<Vec<_>>(),
    })))
}

fn announce(state: &AppState) -> Result<(), Error> {
    let event_id = state
        .db
        .append_event(None, events::PLUGINS_RELOADED, None, &Value::Null)?;
    state
        .reviews
        .publish_plain(event_id, events::PLUGINS_RELOADED);
    Ok(())
}
