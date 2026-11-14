//! The agent runtime in the core: the link to the terminal host, kept while
//! `/labs/runtime` is on. The host itself runs agents and outlives the app;
//! the core starts it, or adopts the one already running, and reconnects
//! when the connection drops.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::Pinrail;

/// The setting that turns the runtime on.
pub const SETTING: &str = "/labs/runtime";

/// The link to the host, when the runtime is on.
#[derive(Default)]
pub struct Runtime {
    #[cfg(unix)]
    link: Mutex<Option<pinrail_host::link::HostLink>>,
    #[cfg(not(unix))]
    link: Mutex<Option<()>>,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime").finish_non_exhaustive()
    }
}

impl Runtime {
    /// The link to the host, while the runtime is on.
    #[cfg(unix)]
    pub fn host(&self) -> Option<pinrail_host::link::HostLink> {
        self.link.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The host's state, for the window and for tests.
    pub fn status(&self) -> Value {
        #[cfg(unix)]
        {
            use pinrail_host::link::HostState;
            let Some(link) = self.host() else {
                return json!({"enabled": false});
            };
            match link.state() {
                HostState::Connecting => json!({"enabled": true, "state": "connecting"}),
                HostState::Ready { instance } => {
                    json!({"enabled": true, "state": "ready", "instance": instance})
                }
                HostState::Reconnecting {
                    attempt,
                    next_in,
                    reason,
                } => json!({"enabled": true, "state": "reconnecting", "attempt": attempt,
                            "next_in": next_in, "reason": reason}),
                HostState::Incompatible { reason } => {
                    json!({"enabled": true, "state": "incompatible", "reason": reason})
                }
                HostState::Offline => json!({"enabled": true, "state": "offline"}),
            }
        }
        #[cfg(not(unix))]
        {
            let _ = &self.link;
            json!({"enabled": false, "unsupported": true})
        }
    }
}

/// Starts the link when the runtime is on, and starts or stops it as the
/// setting changes. Must be called in a runtime.
pub fn watch(app: Arc<Pinrail>) {
    apply(&app);
    let mut notices = app.events().subscribe();
    tokio::spawn(async move {
        while let Ok(notice) = notices.recv().await {
            if notice.kind == crate::events::SETTINGS_CHANGED
                && notice
                    .keys
                    .as_ref()
                    .is_some_and(|keys| keys.iter().any(|k| SETTING.starts_with(k.as_str())))
            {
                apply(&app);
            }
        }
    });
}

/// Starts or stops the link to match the setting. Stopping disconnects;
/// the host and its agents keep running.
fn apply(app: &Pinrail) {
    let on = app.settings().value(SETTING).as_bool().unwrap_or(false);
    #[cfg(unix)]
    {
        let mut link = app.runtime().link.lock().unwrap_or_else(|e| e.into_inner());
        match (on, link.is_some()) {
            (true, false) => *link = Some(start(app)),
            (false, true) => {
                if let Some(link) = link.take() {
                    link.stop();
                }
            }
            _ => {}
        }
    }
    #[cfg(not(unix))]
    let _ = on;
}

#[cfg(unix)]
fn start(app: &Pinrail) -> pinrail_host::link::HostLink {
    use pinrail_host::launch::Launcher;
    use pinrail_host::link::{Connector, HostLink};
    let config = app.config();
    let connector = match &config.host_socket {
        Some((socket, secret)) => Connector::Socket {
            socket: socket.clone(),
            secret: secret.clone(),
        },
        None => Connector::Launch(Arc::new(Launcher {
            bundled: config.host_binary.clone().unwrap_or_else(bundled_host),
            data_dir: config.data_dir.clone(),
            env: std::env::vars().collect(),
            methods: Launcher::methods(),
        })),
    };
    HostLink::start(connector, &format!("pinrail-core/{}", crate::VERSION))
}

/// The `pinrail-host` the app carries beside its own binary.
#[cfg(unix)]
fn bundled_host() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("pinrail-host")))
        .unwrap_or_else(|| "pinrail-host".into())
}
