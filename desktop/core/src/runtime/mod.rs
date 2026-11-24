//! The agent runtime in the core: the link to the terminal host, kept while
//! `/labs/runtime` is on. The host itself runs agents and outlives the app;
//! the core starts it, or adopts the one already running, and reconnects
//! when the connection drops. The runs are the agents it starts there.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::Pinrail;
use crate::db::Db;
use crate::events::Bus;

#[cfg(unix)]
mod delivery;
#[cfg(unix)]
mod runs;
mod tokens;
#[cfg(unix)]
pub use runs::{LOCAL, Reconciled, Resume, RunError, Start, TOKEN_VARIABLE};
pub use tokens::{Admitted, Mismatch, digest};

/// The setting that turns the runtime on.
pub const SETTING: &str = "/labs/runtime";

/// The link to the host, when the runtime is on, and the runs it starts.
pub struct Runtime {
    #[cfg(unix)]
    link: Mutex<Option<pinrail_host::link::HostLink>>,
    #[cfg(not(unix))]
    link: Mutex<Option<()>>,
    db: Arc<Db>,
    bus: Bus,
    /// Variables added to every run's environment, from the config.
    #[cfg_attr(not(unix), allow(dead_code))]
    run_env: BTreeMap<String, String>,
    /// The folder of the files attached to sessions, which every run may
    /// read.
    #[cfg_attr(not(unix), allow(dead_code))]
    session_files: std::path::PathBuf,
    #[cfg(unix)]
    spawns: Mutex<runs::Spawns>,
    #[cfg(unix)]
    startable: Mutex<runs::Startable>,
    #[cfg(unix)]
    checking_startable: tokio::sync::Mutex<()>,
    /// A background check of the agents waits to begin.
    #[cfg(unix)]
    startable_waiting: std::sync::atomic::AtomicBool,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime").finish_non_exhaustive()
    }
}

impl Runtime {
    pub(crate) fn new(
        db: Arc<Db>,
        bus: Bus,
        run_env: BTreeMap<String, String>,
        session_files: std::path::PathBuf,
    ) -> Self {
        Runtime {
            link: Mutex::new(None),
            db,
            bus,
            run_env,
            session_files,
            #[cfg(unix)]
            spawns: Mutex::default(),
            #[cfg(unix)]
            startable: Mutex::default(),
            #[cfg(unix)]
            checking_startable: tokio::sync::Mutex::new(()),
            #[cfg(unix)]
            startable_waiting: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// The link to the host, while the runtime is on.
    #[cfg(unix)]
    pub fn host(&self) -> Option<pinrail_host::link::HostLink> {
        self.link.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The runs that match `filters`, latest first.
    pub fn runs(
        &self,
        filters: &crate::db::RunFilters,
    ) -> Result<Vec<crate::db::RunRow>, crate::error::Error> {
        Ok(self.db.runs(filters)?)
    }

    /// The folders agents worked in lately, for the Start an agent dialog:
    /// those Pinrail has seen, and those in the agents' own history under
    /// `history`, the person's home folder.
    pub fn recent_folders(
        &self,
        history: Option<&std::path::Path>,
    ) -> Result<Vec<Value>, crate::error::Error> {
        Ok(crate::folders::recent(&self.db, history)?)
    }

    /// The first task and the extra arguments a run was started with.
    pub fn run_launch(
        &self,
        id: &str,
    ) -> Result<Option<crate::db::RunLaunch>, crate::error::Error> {
        Ok(self.db.run_launch(id)?)
    }

    /// A run, with every session it has been in.
    pub fn run(
        &self,
        id: &str,
    ) -> Result<Option<(crate::db::RunRow, Vec<crate::db::RunSession>)>, crate::error::Error> {
        let Some(run) = self.db.run(id)? else {
            return Ok(None);
        };
        Ok(Some((run, self.db.run_sessions(id)?)))
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
    cli_control(&app);
    let mut notices = app.events().subscribe();
    tokio::spawn(async move {
        while let Ok(notice) = notices.recv().await {
            if notice.kind != crate::events::SETTINGS_CHANGED {
                continue;
            }
            let changed = |setting: &str| {
                notice
                    .keys
                    .as_ref()
                    .is_some_and(|keys| keys.iter().any(|k| setting.starts_with(k.as_str())))
            };
            if changed(SETTING) {
                apply(&app);
            }
            if changed(crate::person_token::SETTING) {
                cli_control(&app);
            }
        }
    });
}

/// Writes the person token for the CLI while `/runtime/cli_control` is
/// on, and removes it when it is off.
fn cli_control(app: &Pinrail) {
    let on = app
        .settings()
        .value(crate::person_token::SETTING)
        .as_bool()
        .unwrap_or(false);
    if let Err(error) = crate::person_token::sync(app.config(), on) {
        eprintln!("pinrail: writing the person token for the CLI failed: {error}");
    }
}

/// Starts or stops the link to match the setting. Stopping disconnects;
/// the host and its agents keep running.
fn apply(app: &Arc<Pinrail>) {
    let on = app.settings().value(SETTING).as_bool().unwrap_or(false);
    #[cfg(unix)]
    {
        let mut link = app.runtime().link.lock().unwrap_or_else(|e| e.into_inner());
        match (on, link.is_some()) {
            (true, false) => {
                let started = start(app);
                tokio::spawn(follow(app.clone(), started.clone()));
                *link = Some(started);
            }
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
            env: host_env(std::env::vars()),
            methods: Launcher::methods(),
        })),
    };
    HostLink::start(connector, &format!("pinrail-core/{}", crate::VERSION))
}

/// Follows the host until the link stops: reconciles the runs with the
/// host's terminals on every connect, ends the run of a terminal that
/// exits, and forgets the tokens of runs that ended long enough ago.
#[cfg(unix)]
async fn follow(app: Arc<Pinrail>, link: pinrail_host::link::HostLink) {
    use pinrail_host::link::{HostEvent, HostState};
    use tokio::sync::broadcast::error::RecvError;
    let mut events = link.events();
    let mut state = link.watch();
    // a link that connected before this listened
    if matches!(link.state(), HostState::Ready { .. }) {
        reconcile_listed(&app, &link).await;
        check_startable_later(app.clone(), just_before());
    }
    let mut sweep = tokio::time::interval(std::time::Duration::from_secs(60));
    // typed messages unconfirmed after 10 seconds, held ones after 30 minutes
    let mut messages = tokio::time::interval(std::time::Duration::from_secs(2));
    loop {
        tokio::select! {
            event = events.recv() => match event {
                Ok(HostEvent::Connected { terminals, .. }) => {
                    reconcile(&app, &link, &terminals).await;
                    // the agents this host finds, for the Start an agent
                    // dialog; a connect can be seen twice, as a ready
                    // link and as its event, and checks once
                    check_startable_later(app.clone(), just_before());
                }
                Ok(HostEvent::Event { method, params }) if method == "program_status" => {
                    let now = chrono::Utc::now();
                    match app.runtime().program_status(&params, now) {
                        Ok(Some((agent, session, report))) => {
                            if let Err(error) =
                                app.reviews().report(&agent, &session, &report, false, now)
                            {
                                eprintln!("pinrail: recording a program status failed: {error:?}");
                            }
                            // the work confirms the message just typed
                            if report.event == crate::db::ReportEvent::Working
                                && let Err(error) = app.runtime().confirm_by_work(&agent, &session)
                            {
                                eprintln!("pinrail: confirming a typed message failed: {error}");
                            }
                            // an agent that went idle takes the next message
                            let app = app.clone();
                            tokio::spawn(async move {
                                let _ = app.runtime().deliver(&agent, &session).await;
                            });
                        }
                        Ok(None) => {}
                        Err(error) => eprintln!("pinrail: reading a program status failed: {error}"),
                    }
                }
                Ok(HostEvent::Event { method, params }) if method == "notify" => {
                    let now = chrono::Utc::now();
                    match app.runtime().notification(&params, now) {
                        Ok(Some((agent, session, report))) => {
                            if let Err(error) =
                                app.reviews().report(&agent, &session, &report, false, now)
                            {
                                eprintln!("pinrail: recording a notification failed: {error:?}");
                            }
                        }
                        Ok(None) => {}
                        Err(error) => eprintln!("pinrail: reading a notification failed: {error}"),
                    }
                }
                Ok(HostEvent::Event { method, params }) if method == "exited" => {
                    match app.runtime().exited(&params) {
                        Ok(Some(run)) => {
                            report_exit(&app, &run);
                            // messages waiting to be typed go to the Queue
                            deliver_later(&app, &run);
                        }
                        Ok(None) => {}
                        Err(error) => eprintln!("pinrail: ending a run failed: {error}"),
                    }
                }
                Ok(_) => {}
                // events were missed: the host's list says where things are
                Err(RecvError::Lagged(_)) => reconcile_listed(&app, &link).await,
                Err(RecvError::Closed) => break,
            },
            changed = state.changed() => {
                if changed.is_err() || *state.borrow() == HostState::Offline {
                    break;
                }
            }
            _ = messages.tick() => {
                if let Err(error) = app.runtime().sweep_messages() {
                    eprintln!("pinrail: sweeping the messages failed: {error}");
                }
            }
            _ = sweep.tick() => {
                if let Err(error) = app.runtime().forget_tokens() {
                    eprintln!("pinrail: forgetting run tokens failed: {error}");
                }
            }
        }
    }
}

/// Asks the host again which agents it finds, in the background, unless
/// a check began at `since` or later. One check waiting to begin covers
/// every request made before it begins.
#[cfg(unix)]
pub fn check_startable_later(app: Arc<Pinrail>, since: chrono::DateTime<chrono::Utc>) {
    if app
        .runtime()
        .startable_waiting
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return;
    }
    tokio::spawn(async move {
        let _ = app.runtime().check_startable(Some(since)).await;
    });
}

/// A second ago: a check begun since covers a connect seen now.
#[cfg(unix)]
fn just_before() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now() - chrono::TimeDelta::seconds(1)
}

#[cfg(unix)]
async fn reconcile_listed(app: &Pinrail, link: &pinrail_host::link::HostLink) {
    if let Ok(terminals) = link.call("terminal.list", json!({})).await {
        reconcile(app, link, &terminals).await;
    }
}

/// Reconciles the runs with `terminals`, and stops the terminals of runs
/// Pinrail no longer knows as live.
#[cfg(unix)]
async fn reconcile(app: &Pinrail, link: &pinrail_host::link::HostLink, terminals: &Value) {
    let reconciled = match app.runtime().reconcile(terminals) {
        Ok(reconciled) => reconciled,
        Err(error) => {
            eprintln!("pinrail: reconciling the runs failed: {error}");
            return;
        }
    };
    for run in &reconciled.ended {
        report_exit(app, run);
    }
    for terminal in reconciled.orphans {
        let _ = link
            .call(
                "terminal.stop",
                json!({"terminal": terminal, "grace_ms": 2_000}),
            )
            .await;
    }
}

/// Delivers the messages waiting for the run's session in the background.
#[cfg(unix)]
fn deliver_later(app: &Arc<Pinrail>, run: &crate::db::RunRow) {
    if let Some((agent, session)) = run.session.clone() {
        let app = app.clone();
        tokio::spawn(async move {
            let _ = app.runtime().deliver(&agent, &session).await;
        });
    }
}

/// Marks the session of a run whose agent exited as stopped.
#[cfg(unix)]
fn report_exit(app: &Pinrail, run: &crate::db::RunRow) {
    let reason = match run.end_reason.as_deref() {
        Some("restart") => "restart",
        Some("exited" | "stopped") => "exited",
        // lost with its host, or its resume failed after the spawn: the
        // agent is gone all the same, and the session can be resumed
        Some(_) if run.terminal.is_some() => "exited",
        _ => return,
    };
    let Some((agent, session)) = &run.session else {
        return;
    };
    let now = chrono::Utc::now();
    let report = crate::db::Report {
        event: crate::db::ReportEvent::Ended {
            reason: Some(reason.to_string()),
        },
        at: now,
        process: run.process.clone(),
        origin: Default::default(),
        transcript_path: None,
        agent_turn: None,
        cut: false,
        source: crate::db::Source::Host,
    };
    if let Err(error) = app.reviews().report(agent, session, &report, false, now) {
        eprintln!("pinrail: marking session {session} stopped failed: {error:?}");
    }
}

#[cfg(any(unix, test))]
/// The environment the host and the agents it starts get: the app's, and
/// in a debug build started by `cargo run` (`tauri dev`), the app's without
/// what cargo, rustup, the Tauri CLI and npm set for that run. Left in,
/// they make every cargo build an agent runs rebuild what a build from a
/// terminal made, and the reverse, and pin rustup's toolchain in other
/// projects. A release build hands on its environment as it is.
fn host_env(vars: impl Iterator<Item = (String, String)>) -> Vec<(String, String)> {
    let vars: Vec<(String, String)> = vars.collect();
    if !cfg!(debug_assertions) || !vars.iter().any(|(k, _)| k == "CARGO_MANIFEST_DIR") {
        return vars;
    }
    let set_by_cargo = |key: &str| {
        matches!(
            key,
            "CARGO"
                | "CARGO_MANIFEST_DIR"
                | "CARGO_MANIFEST_PATH"
                | "CARGO_MANIFEST_LINKS"
                | "CARGO_PRIMARY_PACKAGE"
                | "CARGO_BIN_NAME"
                | "CARGO_CRATE_NAME"
                | "CARGO_RUSTC_CURRENT_DIR"
                | "DYLD_FALLBACK_LIBRARY_PATH"
                | "RUSTUP_TOOLCHAIN"
                | "RUSTUP_TOOLCHAIN_SOURCE"
                | "RUST_RECURSION_COUNT"
                | "INIT_CWD"
        ) || key.starts_with("CARGO_PKG_")
            || key.starts_with("CARGO_TERM_PROGRESS_")
            || key.starts_with("TAURI_")
            || key.starts_with("npm_")
    };
    vars.into_iter().filter(|(k, _)| !set_by_cargo(k)).collect()
}

/// The `pinrail-host` the app carries.
#[cfg(unix)]
fn bundled_host() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| host_beside(dir, &|p| p.is_file())))
        .unwrap_or_else(|| "pinrail-host".into())
}

/// The host's executable for an app whose binary is in `dir`. On macOS
/// the host is `Pinrail Host.app`: in `Contents/Helpers` of the app's
/// bundle, or beside a development build's binary. Elsewhere, and when
/// there is no bundle, it is `pinrail-host` beside the app's binary.
#[cfg(unix)]
fn host_beside(
    dir: &std::path::Path,
    exists: &dyn Fn(&std::path::Path) -> bool,
) -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    {
        let executable = std::path::Path::new("Pinrail Host.app/Contents/MacOS/pinrail-host");
        let in_bundle = dir.join("../Helpers").join(executable);
        let beside = dir.join(executable);
        for candidate in [in_bundle, beside] {
            if exists(&candidate) {
                return candidate;
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = exists;
    dir.join("pinrail-host")
}

#[cfg(test)]
mod host_env_tests {
    use super::host_env;

    fn vars(pairs: &[(&str, &str)]) -> impl Iterator<Item = (String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn a_debug_build_started_by_cargo_hands_on_none_of_its_build_variables() {
        let env = host_env(vars(&[
            ("PATH", "/usr/bin"),
            ("HOME", "/Users/p"),
            ("CARGO_HOME", "/Users/p/.cargo"),
            ("CARGO_TARGET_DIR", "/tmp/target"),
            ("RUSTUP_HOME", "/Users/p/.rustup"),
            ("CARGO", "/bin/cargo"),
            ("CARGO_MANIFEST_DIR", "/work/app/src-tauri"),
            ("CARGO_MANIFEST_PATH", "/work/app/src-tauri/Cargo.toml"),
            ("CARGO_PKG_NAME", "pinrail-desktop"),
            ("CARGO_TERM_PROGRESS_WHEN", "always"),
            ("RUSTUP_TOOLCHAIN", "1.98.1"),
            ("RUST_RECURSION_COUNT", "1"),
            ("DYLD_FALLBACK_LIBRARY_PATH", "/work/target/debug"),
            ("TAURI_ENV_TARGET_TRIPLE", "aarch64-apple-darwin"),
            ("npm_config_local_prefix", "/work/app"),
            ("INIT_CWD", "/work/app"),
        ]));
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            [
                "PATH",
                "HOME",
                "CARGO_HOME",
                "CARGO_TARGET_DIR",
                "RUSTUP_HOME"
            ]
        );
    }

    #[test]
    fn an_app_not_started_by_cargo_hands_on_its_environment_as_it_is() {
        let given = [
            ("PATH", "/usr/bin"),
            ("RUSTUP_TOOLCHAIN", "nightly"),
            ("DYLD_FALLBACK_LIBRARY_PATH", "/opt/lib"),
        ];
        assert_eq!(host_env(vars(&given)), vars(&given).collect::<Vec<_>>());
    }
}

#[cfg(all(test, unix))]
mod host_beside_tests {
    use super::host_beside;
    use std::path::{Path, PathBuf};

    fn found(dir: &str, existing: &[&str]) -> PathBuf {
        let existing: Vec<PathBuf> = existing.iter().map(PathBuf::from).collect();
        host_beside(Path::new(dir), &|p| existing.iter().any(|e| e == p))
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_host_bundle_is_found_in_the_app_s_helpers() {
        let helper =
            "/A/Pinrail.app/Contents/MacOS/../Helpers/Pinrail Host.app/Contents/MacOS/pinrail-host";
        assert_eq!(
            found("/A/Pinrail.app/Contents/MacOS", &[helper]),
            PathBuf::from(helper)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_host_bundle_is_found_beside_a_development_build() {
        let beside = "/w/target/debug/Pinrail Host.app/Contents/MacOS/pinrail-host";
        assert_eq!(found("/w/target/debug", &[beside]), PathBuf::from(beside));
    }

    #[test]
    fn without_a_bundle_the_host_is_the_binary_beside_the_app() {
        assert_eq!(
            found("/usr/lib/pinrail", &[]),
            PathBuf::from("/usr/lib/pinrail/pinrail-host")
        );
    }
}
