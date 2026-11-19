//! The core links to the terminal host while `/labs/runtime` is on, and
//! lets it be when it is turned off: the host and its agents keep running.
//! Through the link it starts agents as runs, ends a run when its agent
//! exits, and on every connect brings the runs in line with the host's
//! terminals.
#![cfg(unix)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pinrail_core::db::{Db, NewRun, RunRow};
use pinrail_core::runtime::{RunError, Start};
use pinrail_core::{Config, Pinrail, runtime};
use pinrail_host::client::Client;
use pinrail_host::registration::Registration;
use serde_json::{Value, json};

use common::host;

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
        let mut config = Config::new(dir.path().join("data"), 0);
        config.host_socket = Some((registration.socket.clone(), registration.secret.clone()));
        config
            .run_env
            .insert("PATH".into(), common::stand_in_path(dir.path()));
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
        task: None,
        args: &[],
        created_by: "app",
        token_digest: id,
        session: None,
        checkout: None,
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

#[tokio::test(flavor = "multi_thread")]
async fn after_a_restart_the_lost_runs_end_as_restart_and_their_sessions_are_listed() {
    let setup = Setup::new();
    let db = setup.db();
    let now = chrono::Utc::now();
    let new_run = |id: &'static str, session: &'static str| NewRun {
        id,
        host: "local",
        agent: "claude-code",
        kind: "start",
        folder: "/",
        argv: &[],
        task: None,
        args: &[],
        created_by: "app",
        token_digest: id,
        session: Some(session),
        checkout: None,
    };
    db.insert_run(&new_run("run_a", "s-a"), now).unwrap();
    db.run_spawned("run_a", "t_a", None).unwrap();
    db.insert_run(&new_run("run_b", "s-b"), now).unwrap();
    db.run_spawned("run_b", "t_b", None).unwrap();
    // the core last saw this computer in another boot
    db.set_host_boot("local", "an-earlier-boot").unwrap();

    setup.turn_on().await;
    for id in ["run_a", "run_b"] {
        let run = run_becomes(&db, id, |r| r.state == "ended").await;
        assert_eq!(run.end_reason.as_deref(), Some("restart"), "{id}");
    }
    let presence = db.presence("claude-code", "s-a").unwrap().unwrap();
    assert_eq!(presence.state, "stopped");
    assert_eq!(presence.stop_reason.as_deref(), Some("restart"));

    let runtime = setup.app.runtime();
    let mut listed: Vec<String> = runtime
        .restarted()
        .unwrap()
        .into_iter()
        .map(|(r, _)| r.id)
        .collect();
    listed.sort();
    assert_eq!(listed, ["run_a", "run_b"]);
    // the boot is the current one now, so the next connect is no restart
    assert_eq!(db.host_boot("local").unwrap(), pinrail_host::boot_id());

    // dismissed, the list is empty until the next restart
    runtime.dismiss_restart().unwrap();
    assert!(runtime.restarted().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_resumed_after_a_restart_leaves_the_list() {
    let setup = Setup::new();
    setup.turn_on().await;
    let db = setup.db();
    let folder = setup.folder("app");
    let now = chrono::Utc::now();
    db.insert_run(
        &NewRun {
            id: "run_lost",
            host: "local",
            agent: "claude-code",
            kind: "start",
            folder: &folder.to_string_lossy(),
            argv: &[],
            task: None,
            args: &[],
            created_by: "app",
            token_digest: "d",
            session: Some("s-lost"),
            checkout: None,
        },
        now,
    )
    .unwrap();
    db.end_run("run_lost", "restart", None, None, now).unwrap();
    let runtime = setup.app.runtime();
    assert_eq!(runtime.restarted().unwrap().len(), 1);

    runtime
        .resume(&runtime::Resume {
            agent: "claude-code".into(),
            session: "s-lost".into(),
            cols: 80,
            rows: 24,
            created_by: "app".into(),
            anyway: false,
        })
        .await
        .unwrap();
    assert!(runtime.restarted().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_is_typed_into_a_run_and_refused_once_the_run_ended() {
    let setup = Setup::new();
    setup.turn_on().await;
    let folder = setup.folder("app");
    std::fs::write(folder.join("listen"), "").unwrap();
    let runtime = setup.app.runtime();
    let run = runtime.start(&start("claude-code", &folder)).await.unwrap();
    file(folder.join("token")).await;

    runtime
        .type_message(&run.id, "use the v2 endpoint\x1b[201~ now", None, false)
        .await
        .unwrap();
    let heard = file(folder.join("heard")).await;
    // pasted as one bracketed paste, its end marker taken out of the text
    assert!(
        heard.contains("\x1b[200~use the v2 endpoint now\x1b[201~"),
        "{heard:?}"
    );

    runtime.stop(&run.id, true).await.unwrap();
    run_becomes(&setup.db(), &run.id, |r| r.state == "ended").await;
    let refused = runtime
        .type_message(&run.id, "anyone?", None, false)
        .await
        .unwrap_err();
    assert_eq!(refused.code(), "exited");
}

/// Has the stand-in agent in `folder` write `record` as an OSC 7501 record,
/// and waits until it has.
async fn record(folder: &Path, record: &str) {
    std::fs::write(folder.join("record.part"), record).unwrap();
    std::fs::rename(folder.join("record.part"), folder.join("record")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while folder.join("record").exists() {
        assert!(Instant::now() < deadline, "the record was not written");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The session's presence once `done` holds for it.
async fn presence_becomes(
    db: &Db,
    (agent, session): &(String, String),
    done: impl Fn(&pinrail_core::db::PresenceRow) -> bool,
) -> pinrail_core::db::PresenceRow {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let presence = db.presence(agent, session).unwrap();
        if let Some(presence) = presence.as_ref().filter(|p| done(p)) {
            return presence.clone();
        }
        assert!(Instant::now() < deadline, "{presence:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A report of the session's hooks, as `pinrail hook` sends it.
fn hook(app: &Pinrail, (agent, session): &(String, String), event: pinrail_core::db::ReportEvent) {
    let now = chrono::Utc::now();
    let report = pinrail_core::db::Report {
        event,
        at: now,
        process: None,
        origin: Default::default(),
        transcript_path: None,
        agent_turn: None,
        cut: false,
        source: pinrail_core::db::Source::Hook,
    };
    app.reviews()
        .report(agent, session, &report, true, now)
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hosted_run_takes_its_state_from_its_terminal_once_it_reports_there() {
    use pinrail_core::db::ReportEvent;
    let setup = Setup::new();
    setup.turn_on().await;
    let folder = setup.folder("app");
    std::fs::write(folder.join("records"), "").unwrap();
    let run = setup
        .app
        .runtime()
        .start(&start("claude-code", &folder))
        .await
        .unwrap();
    file(folder.join("token")).await;
    let session = run.session.clone().unwrap();
    let db = setup.db();

    // before its terminal reports, its hooks set the state
    hook(&setup.app, &session, ReportEvent::Working);
    assert_eq!(
        db.presence(&session.0, &session.1).unwrap().unwrap().state,
        "working"
    );

    // the terminal's records set it from then on
    record(&folder, "state=idle:app=claude-code").await;
    let idle = presence_becomes(&db, &session, |p| p.state == "idle").await;
    assert_eq!(idle.source, "terminal");
    hook(
        &setup.app,
        &session,
        ReportEvent::Prompt {
            prompt: Some("fix the login".into()),
        },
    );
    let presence = db.presence(&session.0, &session.1).unwrap().unwrap();
    assert_eq!(presence.state, "idle", "a hook no longer sets the state");
    // but its prompt opens a turn
    assert_eq!(presence.turn, 1);

    record(&folder, "state=working:app=claude-code").await;
    presence_becomes(&db, &session, |p| p.state == "working").await;
    // "answer: Red or blue?"
    record(
        &folder,
        "state=blocked:kind=question:app=claude-code:msg=YW5zd2VyOiBSZWQgb3IgYmx1ZT8=",
    )
    .await;
    let waiting = presence_becomes(&db, &session, |p| p.state == "needs_input").await;
    assert_eq!(waiting.wait_kind.as_deref(), Some("question"));
    assert_eq!(
        waiting.wait_summary.as_deref(),
        Some("answer: Red or blue?")
    );

    // a subagent's record leaves the session as it is
    record(&folder, "state=working:app=claude-code:id=agent-1").await;
    record(&folder, "state=done:app=claude-code:id=agent-1").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        db.presence(&session.0, &session.1).unwrap().unwrap().state,
        "needs_input"
    );

    // the end of the turn finishes it
    record(&folder, "state=working:app=claude-code").await;
    record(&folder, "state=done:app=claude-code").await;
    let done = presence_becomes(&db, &session, |p| p.state == "idle").await;
    assert!(done.finished_at.is_some());

    // once the run is stopped, the session's hooks set its state again, as
    // for a session in the person's own terminal
    setup.app.runtime().stop(&run.id, true).await.unwrap();
    presence_becomes(&db, &session, |p| p.state == "stopped").await;
    hook(&setup.app, &session, ReportEvent::Working);
    assert_eq!(
        db.presence(&session.0, &session.1).unwrap().unwrap().state,
        "working"
    );
}

/// Runs git in `folder`, which must succeed.
fn git(folder: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_s_session_takes_the_repository_and_branch_of_its_folder() {
    let setup = Setup::new();
    setup.turn_on().await;
    let db = setup.db();

    let checkout = setup.folder("api");
    git(&checkout, &["init", "-q", "-b", "fix/login"]);
    git(
        &checkout,
        &["remote", "add", "origin", "git@github.com:acme/api.git"],
    );
    let run = setup
        .app
        .runtime()
        .start(&start("claude-code", &checkout))
        .await
        .unwrap();
    let (agent, id) = run.session.clone().unwrap();
    // no hook has reported, and the session already knows where it works
    let session = db
        .session(&agent, &id, chrono::Utc::now())
        .unwrap()
        .unwrap();
    assert_eq!(session.repo.as_deref(), Some("acme/api"));
    assert_eq!(session.reference.as_deref(), Some("fix/login"));

    // a folder outside a checkout starts all the same, with no repository
    let plain = setup.folder("plain");
    let run = setup
        .app
        .runtime()
        .start(&start("claude-code", &plain))
        .await
        .unwrap();
    let (agent, id) = run.session.clone().unwrap();
    let session = db
        .session(&agent, &id, chrono::Utc::now())
        .unwrap()
        .unwrap();
    assert_eq!(session.repo, None);
    assert_eq!(session.folder.as_deref(), plain.to_str());
}
