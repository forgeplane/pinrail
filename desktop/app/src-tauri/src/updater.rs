//! Updates from the project's releases. The app looks at start and every few
//! hours, while `updates.check` is on, and downloads a newer version in the
//! background; the download is checked against the key in the app's config
//! and installed when the person restarts Pinrail, or when it quits. A
//! restart never happens on its own: an agent waiting on a review retries
//! until the server is back, but the person chooses when.
//!
//! A `.deb` or `.rpm` install belongs to the package manager, so there the
//! app only says a version is out and where to get it. A development build
//! never looks.

use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// The shell listens for this; the payload is the `Status`.
const EVENT: &str = "pinrail:update";

/// Where a version the app cannot install by itself is downloaded.
const RELEASES: &str = "https://github.com/forgeplane/pinrail/releases/latest";

/// How long after start the first look waits, so it stays out of the way.
const FIRST_LOOK: Duration = Duration::from_secs(20);
/// How often the app looks again while it runs.
const EVERY: chrono::TimeDelta = chrono::TimeDelta::hours(6);
/// How often it asks itself whether it is time: a sleeping laptop stops a
/// timer, so the interval is measured on the wall clock instead.
const TICK: Duration = Duration::from_secs(15 * 60);

/// Where updating stands, as the shell shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Status {
    /// a development build, which never updates
    Unavailable,
    /// not looked yet
    Idle,
    Checking,
    UpToDate {
        checked_at: DateTime<Utc>,
    },
    Downloading {
        version: String,
        percent: Option<u8>,
    },
    /// downloaded and checked; installed at the next restart
    Ready {
        version: String,
        notes: Option<String>,
    },
    /// out, but for the package manager to install
    Available {
        version: String,
        url: String,
    },
    Failed {
        message: String,
        checked_at: DateTime<Utc>,
    },
}

pub struct Updates {
    status: Mutex<Status>,
    /// the downloaded update, waiting for a restart
    ready: Mutex<Option<(Update, Vec<u8>)>>,
    /// one look at a time
    busy: tokio::sync::Mutex<()>,
    last_look: Mutex<Option<DateTime<Utc>>>,
}

impl Updates {
    pub fn new() -> Self {
        let status = if cfg!(debug_assertions) {
            dev_status().unwrap_or(Status::Unavailable)
        } else {
            Status::Idle
        };
        Self {
            status: Mutex::new(status),
            ready: Mutex::new(None),
            busy: tokio::sync::Mutex::new(()),
            last_look: Mutex::new(None),
        }
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    fn set(&self, app: &AppHandle, status: Status) {
        *self.status.lock().unwrap() = status.clone();
        let offers = matches!(status, Status::Ready { .. } | Status::Available { .. });
        let _ = app.emit(EVENT, status);
        // the tray's menu offers the restart, or the download
        if offers {
            crate::native::refresh_tray(app);
        }
    }
}

/// A development build shows a found update when told to, so the notice,
/// the tray's item and About can be seen without a release:
/// `PINRAIL_DEV_UPDATE=0.2.0` for one ready to install, `available:0.2.0`
/// for one a package manager installs. Restarting then says nothing is
/// downloaded.
fn dev_status() -> Option<Status> {
    let wanted = std::env::var("PINRAIL_DEV_UPDATE").ok()?;
    Some(match wanted.strip_prefix("available:") {
        Some(version) => Status::Available {
            version: version.into(),
            url: RELEASES.into(),
        },
        None => Status::Ready {
            version: wanted,
            notes: None,
        },
    })
}

/// Looks now and downloads what it finds; what `check_for_updates` runs,
/// and the timer too. A downloaded update stands until the restart.
pub async fn check(app: &AppHandle) -> Status {
    let updates = app.state::<Updates>();
    if cfg!(debug_assertions) {
        return Status::Unavailable;
    }
    let _one = updates.busy.lock().await;
    *updates.last_look.lock().unwrap() = Some(Utc::now());
    if matches!(updates.status(), Status::Ready { .. }) {
        return updates.status();
    }
    updates.set(app, Status::Checking);
    let status = match look(app, &updates).await {
        Ok(status) => status,
        Err(message) => Status::Failed {
            message,
            checked_at: Utc::now(),
        },
    };
    updates.set(app, status.clone());
    status
}

async fn look(app: &AppHandle, updates: &Updates) -> Result<Status, String> {
    let found = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let Some(update) = found else {
        return Ok(Status::UpToDate {
            checked_at: Utc::now(),
        });
    };
    if !installs_itself() {
        return Ok(Status::Available {
            version: update.version.clone(),
            url: RELEASES.into(),
        });
    }
    let version = update.version.clone();
    updates.set(
        app,
        Status::Downloading {
            version: version.clone(),
            percent: Some(0),
        },
    );
    let mut received = 0usize;
    let mut shown = 0u8;
    let bytes = update
        .download(
            |chunk, total| {
                received += chunk;
                let percent = total
                    .filter(|t| *t > 0)
                    .map(|t| (received as u64 * 100 / t).min(100) as u8);
                // a step of five is plenty for a progress line
                if let Some(p) = percent.filter(|p| *p >= shown + 5) {
                    shown = p;
                    updates.set(
                        app,
                        Status::Downloading {
                            version: version.clone(),
                            percent: Some(p),
                        },
                    );
                }
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    let notes = update.body.clone().filter(|n| !n.trim().is_empty());
    *updates.ready.lock().unwrap() = Some((update, bytes));
    Ok(Status::Ready { version, notes })
}

/// Whether this copy can replace itself: everything but a Linux package,
/// which the package manager owns.
fn installs_itself() -> bool {
    if !cfg!(target_os = "linux") {
        return true;
    }
    matches!(
        tauri::utils::platform::bundle_type(),
        Some(tauri::utils::config::BundleType::AppImage)
    )
}

/// Installs the downloaded update, if there is one; true when it did. The
/// new version runs from the next start.
pub fn install(app: &AppHandle) -> Result<bool, String> {
    let Some((update, bytes)) = app.state::<Updates>().ready.lock().unwrap().take() else {
        return Ok(false);
    };
    update.install(bytes).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Installs the downloaded update and starts the new version; from the
/// window's *Restart to update* and the tray's.
pub fn restart(app: &AppHandle) -> Result<(), String> {
    if !install(app)? {
        return Err("no update is downloaded".into());
    }
    app.restart()
}

/// Looks shortly after start, then every few hours, while `updates.check`
/// is on. A development build does not.
pub fn start(app: &AppHandle, enabled: impl Fn() -> bool + Send + 'static) {
    if cfg!(debug_assertions) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_LOOK).await;
        loop {
            let due = app
                .state::<Updates>()
                .last_look
                .lock()
                .unwrap()
                .is_none_or(|at| Utc::now() - at >= EVERY);
            if due && enabled() {
                check(&app).await;
            }
            tokio::time::sleep(TICK).await;
        }
    });
}
