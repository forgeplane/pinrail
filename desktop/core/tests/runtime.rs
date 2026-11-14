//! The core links to the terminal host while `/labs/runtime` is on, and
//! lets it be when it is turned off: the host and its agents keep running.
//! Through the link it starts agents as runs, ends a run when its agent
//! exits, and on every connect brings the runs in line with the host's
//! terminals.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pinrail_core::db::{Db, NewRun, RunRow};
use pinrail_core::runtime::{RunError, Start};
use pinrail_core::{Config, Pinrail, runtime};
use pinrail_host::client::Client;
use pinrail_host::registration::Registration;
use serde_json::{Value, json};

/// A host served from a thread of the test, and its registration.
fn host(dir: &std::path::Path) -> Registration {
    let data_dir = dir.join("host-data");
    let config = pinrail_host::server::Config {
        data_dir: data_dir.clone(),
        idle_exit: Duration::from_secs(600),
        ping_every: Duration::from_secs(15),
    };
    std::thread::spawn(move || pinrail_host::server::serve(config));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(registration) = Registration::read(&data_dir.join("host/host.json")) {
            return registration;
        }
        assert!(Instant::now() < deadline, "the host did not start");
        std::thread::sleep(Duration::from_millis(20));
    }
}

async fn status_becomes(app: &Pinrail, state: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = app.runtime().status();
        if status["state"] == state {
            return status;
        }
        assert!(Instant::now() < deadline, "{status}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_core_links_to_the_host_while_the_runtime_is_on() {
    let dir = tempfile::tempdir().unwrap();
    let registration = host(dir.path());
    let mut config = Config::new(dir.path().join("data"), 0);
    config.host_socket = Some((registration.socket.clone(), registration.secret.clone()));
    let app = Arc::new(Pinrail::open(config).unwrap());
    runtime::watch(app.clone());
    // off by default
    assert_eq!(app.runtime().status(), json!({"enabled": false}));

    app.settings()
        .change(&json!({"labs": {"runtime": true}}))
        .unwrap();
    let status = status_becomes(&app, "ready").await;
    assert!(status["instance"].is_string());
    let link = app.runtime().host().unwrap();
    assert_eq!(
        link.call("host.info", json!({})).await.unwrap()["terminals"],
        0
    );

    // turning it off disconnects, and the host keeps running
    app.settings()
        .change(&json!({"labs": {"runtime": false}}))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.runtime().status() != json!({"enabled": false}) {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let still = tokio::task::spawn_blocking(move || Client::connect(&registration).is_ok())
        .await
        .unwrap();
    assert!(still);
}

/// A stand-in for Claude Code: it writes its arguments and its run token
/// into the folder it starts in, then exits with the code in the folder's
/// `exit` file, or waits.
const AGENT: &str = r#"#!/bin/sh
printf '%s\n' "$@" > argv
printf '%s' "$PINRAIL_RUN_TOKEN" > token
if [ -f exit ]; then exit "$(cat exit)"; fi
exec sleep 600
"#;

struct Setup {
    app: Arc<Pinrail>,
    registration: Registration,
    dir: tempfile::TempDir,
}

impl Setup {
    /// A host, and a core whose runs find `claude` in a folder of the
    /// test's, linked to it once the runtime is turned on.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let registration = host(dir.path());
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let claude = bin.join("claude");
        std::fs::write(&claude, AGENT).unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut config = Config::new(dir.path().join("data"), 0);
        config.host_socket = Some((registration.socket.clone(), registration.secret.clone()));
        config
            .run_env
            .insert("PATH".into(), format!("{}:/usr/bin:/bin", bin.display()));
        let app = Arc::new(Pinrail::open(config).unwrap());
        Setup {
            app,
            registration,
            dir,
        }
    }

    async fn turn_on(&self) {
        runtime::watch(self.app.clone());
        self.app
            .settings()
            .change(&json!({"labs": {"runtime": true}}))
            .unwrap();
        status_becomes(&self.app, "ready").await;
    }

    /// A folder to start an agent in.
    fn folder(&self, name: &str) -> PathBuf {
        let folder = self.dir.path().join(name);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn db(&self) -> Db {
        Db::open(&self.app.config().db_path()).unwrap()
    }

    fn client(&self) -> Client {
        Client::connect(&self.registration).unwrap()
    }
}

fn start(agent: &str, folder: &Path) -> Start {
    Start {
        agent: agent.to_string(),
        folder: folder.to_string_lossy().into_owned(),
        task: Some("fix the login".to_string()),
        args: vec![],
        cols: 80,
        rows: 24,
        created_by: "app".to_string(),
    }
}

/// The run once `done` holds for it.
async fn run_becomes(db: &Db, id: &str, done: impl Fn(&RunRow) -> bool) -> RunRow {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let run = db.run(id).unwrap().unwrap();
        if done(&run) {
            return run;
        }
        assert!(Instant::now() < deadline, "{run:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn file(path: PathBuf) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(text) = std::fs::read_to_string(&path)
            && !text.is_empty()
        {
            return text;
        }
        assert!(
            Instant::now() < deadline,
            "{} was not written",
            path.display()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_starts_its_agent_with_a_token_and_ends_when_it_exits() {
    let setup = Setup::new();
    setup.turn_on().await;
    let folder = setup.folder("app");
    std::fs::write(folder.join("exit"), "3").unwrap();

    let run = setup
        .app
        .runtime()
        .start(&start("claude-code", &folder))
        .await
        .unwrap();
    let (agent, session) = run.session.clone().unwrap();
    assert_eq!(agent, "claude-code");

    let argv = file(folder.join("argv")).await;
    assert_eq!(
        argv,
        format!("--session-id\n{session}\n--\nfix the login\n")
    );
    // the core keeps only the token's digest
    let token = file(folder.join("token")).await;
    let db = setup.db();
    let found = db
        .run_for_token(&runtime::digest(&token), chrono::Utc::now())
        .unwrap()
        .unwrap();
    assert_eq!(found.id, run.id);

    let run = run_becomes(&db, &run.id, |r| r.state == "ended").await;
    assert_eq!(run.end_reason.as_deref(), Some("exited"));
    assert_eq!(run.exit_code, Some(3));
    assert!(run.terminal.is_some());
    assert!(run.process.is_some());
    let presence = db.presence(&agent, &session).unwrap().unwrap();
    assert_eq!(presence.state, "stopped");
    assert_eq!(presence.stop_reason.as_deref(), Some("exited"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_run_ends_as_stopped() {
    let setup = Setup::new();
    setup.turn_on().await;
    let folder = setup.folder("app");
    let runtime = setup.app.runtime();
    let run = runtime.start(&start("claude-code", &folder)).await.unwrap();
    assert_eq!(run.state, "running");
    file(folder.join("token")).await;

    runtime.stop(&run.id, false).await.unwrap();
    let run = run_becomes(&setup.db(), &run.id, |r| r.state == "ended").await;
    assert_eq!(run.end_reason.as_deref(), Some("stopped"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_the_host_refuses_ends_its_run() {
    let setup = Setup::new();
    let folder = setup.folder("app");
    let runtime = setup.app.runtime();
    // nothing starts while the host is not connected
    assert!(matches!(
        runtime.start(&start("claude-code", &folder)).await,
        Err(RunError::HostOffline)
    ));
    setup.turn_on().await;

    // Codex is not on the runs' PATH
    let error = runtime.start(&start("codex", &folder)).await.unwrap_err();
    assert_eq!(error.code(), "agent_not_found");
    let missing = setup.dir.path().join("missing");
    let error = runtime
        .start(&start("claude-code", &missing))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "folder_missing");

    let runs = setup.db().runs(&Default::default()).unwrap();
    assert_eq!(runs.len(), 2);
    assert!(
        runs.iter()
            .all(|r| r.end_reason.as_deref() == Some("start_failed")),
        "{runs:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_with_a_flag_that_turns_permission_checks_off_is_refused() {
    let setup = Setup::new();
    setup.turn_on().await;
    let mut request = start("claude-code", &setup.folder("app"));
    request.args = vec!["--dangerously-skip-permissions".to_string()];
    let error = setup.app.runtime().start(&request).await.unwrap_err();
    assert_eq!(error.code(), "invalid");
    assert!(setup.db().runs(&Default::default()).unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn on_connect_the_runs_are_reconciled_with_the_hosts_terminals() {
    let setup = Setup::new();
    let db = setup.db();
    let now = chrono::Utc::now();
    let new_run = |id: &'static str| NewRun {
        id,
        host: "local",
        agent: "claude-code",
        kind: "start",
        folder: "/",
        argv: &[],
        created_by: "app",
        token_digest: id,
        session: None,
    };
    // a running run whose terminal the host no longer has
    db.insert_run(&new_run("run_gone"), now).unwrap();
    db.run_spawned("run_gone", "t_gone", None).unwrap();
    // a run whose spawn reply was lost, and the terminal the host made
    db.insert_run(&new_run("run_lost_reply"), now).unwrap();
    // a run whose spawn never reached the host
    db.insert_run(&new_run("run_never"), now).unwrap();

    let mut client = setup.client();
    let mut spawn = |run: &str| {
        client
            .call(
                "terminal.spawn",
                json!({"argv": ["sleep", "600"], "cwd": "/", "cols": 80, "rows": 24,
                       "labels": {"run": run}, "env_base": "host"}),
            )
            .unwrap()["terminal"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let adopted = spawn("run_lost_reply");
    let orphan = spawn("run_unknown");

    setup.turn_on().await;
    let ended = |id: &'static str| {
        let db = &db;
        async move {
            run_becomes(db, id, |r| r.state == "ended")
                .await
                .end_reason
                .unwrap()
        }
    };
    assert_eq!(ended("run_gone").await, "host_lost");
    assert_eq!(ended("run_never").await, "start_failed");
    let run = run_becomes(&db, "run_lost_reply", |r| r.state == "running").await;
    assert_eq!(run.terminal.as_deref(), Some(adopted.as_str()));
    assert!(run.process.is_some());

    // the terminal of a run Pinrail does not know is stopped
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let list = setup.client().call("terminal.list", json!({})).unwrap();
        let entry = list
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["terminal"] == orphan.as_str())
            .cloned()
            .unwrap();
        if entry.get("exit").is_some() {
            break;
        }
        assert!(Instant::now() < deadline, "{entry}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
