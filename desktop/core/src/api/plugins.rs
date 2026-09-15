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
        .route("/api/v1/plugins/install", post(install))
        .route("/api/v1/plugins/{name}/versions", get(versions))
}

async fn index(State(state): State<Arc<AppState>>) -> Json<Value> {
    let stored = state.settings.value(crate::settings::PLUGINS);
    let plugins = state
        .registry
        .all()
        .iter()
        .map(|p| {
            let mut row = p.to_json();
            // the plugin's settings as they stand: defaults under the stored values
            row["settings"] = p
                .has_settings()
                .then(|| p.effective_settings(&stored[&p.name]))
                .into();
            row
        })
        .collect::<Vec<_>>();
    Json(json!({
        "dirs": state.registry.dirs().iter().map(|d| d.display().to_string()).collect::<Vec<_>>(),
        "plugins": plugins,
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

/// Installs one plugin from a folder: `{source, link?, force?}`. Answers
/// with the plugin's row.
async fn install(State(state): State<Arc<AppState>>, body: Bytes) -> Result<Json<Value>, Error> {
    let body = parse_body(&body)?;
    let Some(source) = body.get("source").and_then(Value::as_str) else {
        return Err(Error::invalid("/source", "is required"));
    };
    let options = crate::install::Options {
        link: body.get("link").and_then(Value::as_bool).unwrap_or(false),
        force: body.get("force").and_then(Value::as_bool).unwrap_or(false),
    };
    let record =
        crate::install::install_path(&state.db, &state.registry, FsPath::new(source), options)?;
    announce(&state)?;
    let plugin = state.registry.get(&record.name).ok_or_else(|| {
        Error::Internal(format!(
            "{} was installed and is not registered",
            record.name
        ))
    })?;
    Ok(Json(plugin.to_json()))
}

async fn reload(State(state): State<Arc<AppState>>) -> Result<Json<Value>, Error> {
    let records = state.db.installed_plugins()?;
    let count = state
        .registry
        .reload_with(records)
        .map_err(Error::Internal)?;
    announce(&state)?;
    Ok(Json(json!({ "ok": true, "count": count })))
}

async fn add_dir(State(state): State<Arc<AppState>>, body: Bytes) -> Result<Json<Value>, Error> {
    let body = parse_body(&body)?;
    let Some(dir) = body.get("dir").and_then(Value::as_str) else {
        return Err(Error::invalid("/dir", "is required"));
    };
    // every plugin in the directory becomes a link, served live from where
    // it is; a duplicate name refuses the whole directory and keeps nothing
    let links = state
        .registry
        .link_all(FsPath::new(dir))
        .map_err(|message| Error::invalid("/dir", message))?;
    let mut records = state.db.installed_plugins()?;
    records.extend(links.iter().cloned());
    let count = match state.registry.reload_with(records) {
        Ok(count) => count,
        Err(message) => {
            let _ = state.registry.reload_with(state.db.installed_plugins()?);
            return Err(Error::invalid("/dir", message));
        }
    };
    for record in &links {
        state.db.upsert_installed(record)?;
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
