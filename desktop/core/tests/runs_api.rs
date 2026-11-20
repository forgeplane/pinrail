//! The runs routes: the person starts an agent in a folder, lists and
//! reads its runs, and stops one. A report sent with a run token speaks
//! for the run's session only from the run's own agent process.
#![cfg(unix)]

mod common;

use std::time::{Duration, Instant};

use axum::http::StatusCode;
use pinrail_core::runtime;
use serde_json::{Value, json};

use common::{App, call, call_as};

/// A core linked to a host, whose runs find the stand-in `claude`.
async fn linked() -> App {
    linked_as(&["claude"]).await
}

/// A core linked to a host, whose runs find the stand-in agent as each of
/// `names`.
async fn linked_as(names: &[&str]) -> App {
    let hosts = tempfile::tempdir().unwrap();
    let registration = common::host(hosts.path());
    let path = common::stand_in_path_as(hosts.path(), names);
    let app = common::app_with(|config| {
        config.host_socket = Some((registration.socket.clone(), registration.secret.clone()));
        config.run_env.insert("PATH".into(), path);
    });
    // the host's folder lives as long as the test
    std::mem::forget(hosts);
    runtime::watch(app.state.clone());
    app.state
        .settings()
        .change(&json!({"labs": {"runtime": true}}))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.state.runtime().status()["state"] != "ready" {
        assert!(Instant::now() < deadline, "the host did not connect");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    app
}

fn folder(app: &App, name: &str) -> String {
    let folder = app.dir.path().join(name);
    std::fs::create_dir_all(&folder).unwrap();
    folder.to_string_lossy().into_owned()
}

/// The run once `done` holds for it.
async fn run_becomes(app: &App, id: &str, done: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let (_, run) = call(app, "GET", &format!("/api/v1/runs/{id}"), None).await;
        if done(&run) {
            return run;
        }
        assert!(Instant::now() < deadline, "{run}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn token(folder: &str) -> String {
    let path = std::path::Path::new(folder).join("token");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(token) = std::fs::read_to_string(&path)
            && !token.is_empty()
        {
            return token;
        }
        assert!(Instant::now() < deadline, "the agent wrote no token");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_person_starts_lists_and_stops_a_run() {
    let app = linked().await;
    let folder = folder(&app, "app");
    let body = json!({"host": "local", "agent": "claude-code", "folder": folder,
                      "task": "fix the login", "local_id": "l-1"});

    // only the person starts an agent
    let (status, _) = call_as(&app, "POST", "/api/v1/runs", Some(body.clone()), &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, started) = call(&app, "POST", "/api/v1/runs", Some(body.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    let run = &started["run"];
    let id = run["id"].as_str().unwrap().to_string();
    assert_eq!(run["state"], "running");
    assert!(run["terminal"].as_str().unwrap().starts_with("local:t_"));
    assert_eq!(started["session"]["agent"], "claude-code");
    let session = started["session"]["id"].as_str().unwrap().to_string();

    // the same local_id is the same run
    let (status, again) = call(&app, "POST", "/api/v1/runs", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(again["run"]["id"], id.as_str());

    let (_, live) = call(&app, "GET", "/api/v1/runs?state=live", None).await;
    assert_eq!(live["runs"].as_array().unwrap().len(), 1);
    let (_, of_session) = call(
        &app,
        "GET",
        &format!("/api/v1/runs?session=claude-code/{session}"),
        None,
    )
    .await;
    assert_eq!(of_session["runs"][0]["id"], id.as_str());
    // Pinrail's id alone names the session too
    let (_, by_id) = call(
        &app,
        "GET",
        &format!("/api/v1/runs?session={session}"),
        None,
    )
    .await;
    assert_eq!(by_id["runs"][0]["id"], id.as_str());
    let (_, none) = call(&app, "GET", "/api/v1/runs?session=ses_none", None).await;
    assert_eq!(none["runs"].as_array().unwrap().len(), 0);
    let (_, shown) = call(&app, "GET", &format!("/api/v1/runs/{id}"), None).await;
    assert_eq!(shown["sessions"][0]["id"], session.as_str());

    token(&folder).await;
    let (status, _) = call(&app, "POST", &format!("/api/v1/runs/{id}/stop"), None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let run = run_becomes(&app, &id, |r| r["state"] == "ended").await;
    assert_eq!(run["end_reason"], "stopped");
    let (_, ended) = call(&app, "GET", "/api/v1/runs?state=ended", None).await;
    assert_eq!(ended["runs"][0]["id"], id.as_str());

    let (status, _) = call(&app, "GET", "/api/v1/runs/run_none", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cli_starts_runs_as_its_own_and_reads_the_host() {
    let app = linked().await;
    let folder = folder(&app, "app");
    let bearer = format!("Bearer {}", app.state.config().person_token);
    let cli = [
        ("authorization", bearer.as_str()),
        ("x-pinrail-client", "cli"),
    ];
    let (status, started) = call_as(
        &app,
        "POST",
        "/api/v1/runs",
        Some(json!({"agent": "claude-code", "folder": folder})),
        &cli,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    assert_eq!(started["run"]["created_by"], "cli");

    let (status, host) = call_as(&app, "GET", "/api/v1/hosts/local", None, &cli).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(host["link"]["state"], "ready");
    assert_eq!(host["info"]["terminals"], 1);
    let terminal = started["run"]["terminal"].as_str().unwrap();
    assert_eq!(
        format!(
            "local:{}",
            host["terminals"][0]["terminal"].as_str().unwrap()
        ),
        terminal
    );
    // only the person reads it
    let (status, _) = call_as(&app, "GET", "/api/v1/hosts/local", None, &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call_as(&app, "GET", "/api/v1/hosts/other", None, &cli).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_that_cannot_happen_answers_with_its_reason() {
    let app = linked().await;
    let folder = folder(&app, "app");
    let missing = app
        .dir
        .path()
        .join("missing")
        .to_string_lossy()
        .into_owned();
    for (body, status, kind) in [
        (
            json!({"agent": "gemini", "folder": folder}),
            StatusCode::BAD_REQUEST,
            "invalid",
        ),
        (
            json!({"agent": "claude-code", "folder": folder, "args": ["--yolo"]}),
            StatusCode::BAD_REQUEST,
            "invalid",
        ),
        // Codex is not on the runs' PATH
        (
            json!({"agent": "codex", "folder": folder}),
            StatusCode::NOT_FOUND,
            "agent_not_found",
        ),
        (
            json!({"agent": "claude-code", "folder": missing}),
            StatusCode::UNPROCESSABLE_ENTITY,
            "folder_missing",
        ),
    ] {
        let (got, answer) = call(&app, "POST", "/api/v1/runs", Some(body.clone())).await;
        assert_eq!(
            (got, answer["error"].as_str()),
            (status, Some(kind)),
            "{body}"
        );
    }

    // a flag that turns permission checks off is refused in the spec's words
    let (_, answer) = call(
        &app,
        "POST",
        "/api/v1/runs",
        Some(json!({"agent": "claude-code", "folder": folder,
                    "args": ["--model", "opus", "--dangerously-skip-permissions"]})),
    )
    .await;
    assert_eq!(
        answer["violations"][0]["message"],
        "Pinrail does not start agents with permission checks turned off \
         (--dangerously-skip-permissions).",
        "{answer}"
    );

    app.state
        .settings()
        .change(&json!({"labs": {"runtime": false}}))
        .unwrap();
    // the link closes once the settings event is handled
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.state.runtime().status()["state"] == "ready" {
        assert!(Instant::now() < deadline, "the runtime did not stop");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (status, answer) = call(
        &app,
        "POST",
        "/api/v1/runs",
        Some(json!({"agent": "claude-code", "folder": folder})),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(answer["error"], "host_offline");
}

/// A report of `event` for the session, from `process`, with the run's
/// token.
async fn report(
    app: &App,
    session: &str,
    body: Value,
    process: &(i64, String),
    token: &str,
) -> (StatusCode, Value) {
    let mut body = body;
    body["process"] = json!({"pid": process.0, "started": process.1});
    call_as(
        app,
        "POST",
        &format!("/api/v1/sessions/claude-code/{session}/reports"),
        Some(body),
        &[("authorization", &format!("Run {token}"))],
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_token_speaks_for_the_runs_session_only_from_its_own_process() {
    let app = linked().await;
    let folder = folder(&app, "app");
    let (_, started) = call(
        &app,
        "POST",
        "/api/v1/runs",
        Some(json!({"agent": "claude-code", "folder": folder})),
    )
    .await;
    let id = started["run"]["id"].as_str().unwrap().to_string();
    let first = started["session"]["id"].as_str().unwrap().to_string();
    // the agent names its session by its own id
    let agent_session = started["session"]["agent_session"].as_str().unwrap();
    let token = token(&folder).await;
    let own = common::db(&app).run(&id).unwrap().unwrap().process.unwrap();

    let (status, _) = report(
        &app,
        agent_session,
        json!({"event": "working"}),
        &own,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    // another session's report from the run's process is refused
    let (status, answer) = report(&app, "s-other", json!({"event": "working"}), &own, &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(answer["error"], "run_mismatch");

    // /clear moves the run to a new session, and stops the one it left
    let (status, _) = report(
        &app,
        "s-2",
        json!({"event": "started", "source": "clear"}),
        &own,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let run = run_becomes(&app, &id, |r| r["session"]["agent_session"] == "s-2").await;
    assert_eq!(run["sessions"].as_array().unwrap().len(), 2);
    // a new session of Pinrail's
    let second = run["session"]["id"].as_str().unwrap().to_string();
    assert!(second.starts_with("ses_") && second != first, "{run}");
    let presence = common::db(&app)
        .presence("claude-code", &first)
        .unwrap()
        .unwrap();
    assert_eq!(
        (presence.state.as_str(), presence.stop_reason.as_deref()),
        ("stopped", Some("cleared"))
    );

    // a nested agent that inherited the token neither moves the run nor is
    // refused: its token is ignored
    let nested = (own.0 + 1_000_000, "0".to_string());
    for event in [
        json!({"event": "started", "source": "startup"}),
        json!({"event": "working"}),
    ] {
        let (status, _) = report(&app, "s-nested", event, &nested, &token).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    let (_, run) = call(&app, "GET", &format!("/api/v1/runs/{id}"), None).await;
    assert_eq!(run["session"]["id"], second.as_str());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_claude_code_start_has_pinrails_session_id_and_the_one_it_chose() {
    let app = linked().await;
    let folder = folder(&app, "app");
    let (status, started) = call(
        &app,
        "POST",
        "/api/v1/runs",
        Some(json!({"agent": "claude-code", "folder": folder})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    let session = started["session"]["id"].as_str().unwrap();
    assert!(session.starts_with("ses_"), "{started}");
    // the agent's own id is the one Pinrail passed it
    let argv = argv(&folder).await;
    let chosen = argv
        .lines()
        .skip_while(|a| *a != "--session-id")
        .nth(1)
        .unwrap();
    assert_eq!(started["session"]["agent_session"], chosen, "{started}");
    let (status, shown) = call(
        &app,
        "GET",
        &format!("/api/v1/sessions/claude-code/{session}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{shown}");
    assert_eq!(shown["session"]["agent_session"], chosen);
    assert_eq!(shown["session"]["resume"]["resume"], true);

    // the agent's reports name the session by its own id
    let token = token(&folder).await;
    let id = started["run"]["id"].as_str().unwrap();
    let own = common::db(&app).run(id).unwrap().unwrap().process.unwrap();
    let (status, _) = report(&app, chosen, json!({"event": "working"}), &own, &token).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, shown) = call(
        &app,
        "GET",
        &format!("/api/v1/sessions/claude-code/{session}"),
        None,
    )
    .await;
    assert_eq!(shown["session"]["presence"]["state"], "working", "{shown}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_that_chooses_its_session_id_gives_it_with_its_first_report() {
    let app = linked_as(&["claude", "codex"]).await;
    let folder = folder(&app, "app");
    let (status, started) = call(
        &app,
        "POST",
        "/api/v1/runs",
        Some(json!({"agent": "codex", "folder": folder})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    // the session is there at once, without the agent's id
    let session = started["session"]["id"].as_str().unwrap().to_string();
    assert!(session.starts_with("ses_"), "{started}");
    assert!(started["session"]["agent_session"].is_null(), "{started}");
    let path = format!("/api/v1/sessions/codex/{session}");
    let (status, shown) = call(&app, "GET", &path, None).await;
    assert_eq!(status, StatusCode::OK, "{shown}");
    assert!(shown["session"]["agent_session"].is_null());
    assert_eq!(shown["session"]["resume"]["resume"], false);

    // the run's first report names it
    let id = started["run"]["id"].as_str().unwrap().to_string();
    let token = token(&folder).await;
    let own = common::db(&app).run(&id).unwrap().unwrap().process.unwrap();
    let mut body = json!({"event": "started", "source": "startup",
                          "process": {"pid": own.0, "started": own.1}});
    let (status, answer) = call_as(
        &app,
        "POST",
        "/api/v1/sessions/codex/019a-codex-own/reports",
        Some(body.clone()),
        &[("authorization", &format!("Run {token}"))],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{answer}");
    let (_, shown) = call(&app, "GET", &path, None).await;
    assert_eq!(
        shown["session"]["agent_session"], "019a-codex-own",
        "{shown}"
    );
    assert_eq!(shown["session"]["resume"]["resume"], true);
    let (_, run) = call(&app, "GET", &format!("/api/v1/runs/{id}"), None).await;
    assert_eq!(run["session"]["id"], session.as_str(), "{run}");
    assert_eq!(run["sessions"].as_array().unwrap().len(), 1, "{run}");

    // what the agent sends later, without the token, reaches that session
    let (status, _) = call_as(
        &app,
        "POST",
        "/api/v1/sessions/codex/019a-codex-own/status",
        Some(json!({"text": "Reading the code"})),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, shown) = call(&app, "GET", &path, None).await;
    assert_eq!(shown["statuses"][0]["text"], "Reading the code", "{shown}");

    // another report of another session from the run is refused
    body["event"] = json!("working");
    let (status, answer) = call_as(
        &app,
        "POST",
        "/api/v1/sessions/codex/019a-codex-other/reports",
        Some(body),
        &[("authorization", &format!("Run {token}"))],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    assert_eq!(answer["error"], "run_mismatch");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_whose_agent_never_gave_its_id_is_not_resumed() {
    let app = linked_as(&["claude", "codex"]).await;
    let folder = folder(&app, "app");
    std::fs::write(std::path::Path::new(&folder).join("exit"), "0").unwrap();
    let (_, started) = call(
        &app,
        "POST",
        "/api/v1/runs",
        Some(json!({"agent": "codex", "folder": folder})),
    )
    .await;
    run_becomes(&app, started["run"]["id"].as_str().unwrap(), |r| {
        r["state"] == "ended"
    })
    .await;
    let session = started["session"]["id"].as_str().unwrap();
    let (status, answer) = resume(&app, "codex", session).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    assert_eq!(answer["error"], "not_resumable");
    assert_eq!(
        answer["message"],
        "Pinrail does not know the agent's own id for this session."
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_agents_pinrail_can_start_are_listed_with_their_versions() {
    let hosts = tempfile::tempdir().unwrap();
    let registration = common::host(hosts.path());
    // Claude Code, agy, Grok and Copilot are installed, and Codex is not
    let bin = hosts.path().join("versions");
    common::program(&bin, "claude", "echo '2.1.325 (Claude Code)'");
    common::program(&bin, "agy", "echo '1.5.12'");
    common::program(&bin, "grok", "echo 'grok 1.0.89 (28be5bb1bf65) [stable]'");
    common::program(&bin, "copilot", "echo 'GitHub Copilot CLI 1.0.114.'");
    let app = common::app_with(|config| {
        config.host_socket = Some((registration.socket.clone(), registration.secret.clone()));
        config
            .run_env
            .insert("PATH".into(), format!("{}:/usr/bin:/bin", bin.display()));
    });
    runtime::watch(app.state.clone());
    app.state
        .settings()
        .change(&json!({"labs": {"runtime": true}}))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.state.runtime().status()["state"] != "ready" {
        assert!(Instant::now() < deadline, "the host did not connect");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let (status, body) = call(&app, "GET", "/api/v1/agents/startable", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let agents = body["agents"].as_array().unwrap();
    let agent = |id: &str| agents.iter().find(|a| a["id"] == id).cloned().unwrap();
    assert_eq!(agent("claude-code")["installed"], true);
    assert_eq!(agent("claude-code")["version"], "2.1.325");
    assert_eq!(agent("claude-code")["name"], "Claude Code");
    assert_eq!(agent("codex")["installed"], false);
    assert_eq!(agent("codex")["version"], Value::Null);
    assert_eq!(agent("antigravity")["version"], "1.5.12");
    assert_eq!(agent("antigravity")["name"], "Antigravity CLI");
    assert_eq!(agent("grok")["version"], "1.0.89");
    assert_eq!(agent("grok")["name"], "Grok CLI");
    assert_eq!(agent("copilot")["version"], "1.0.114");
    assert_eq!(agent("copilot")["name"], "Copilot CLI");
    assert_eq!(agent("copilot")["names_session"], true);
    // Pinrail chooses Claude Code's session id; agy tells it with a hook
    assert_eq!(agent("claude-code")["names_session"], true);
    assert_eq!(agent("antigravity")["names_session"], false);
    assert_eq!(agent("grok")["names_session"], true);
    // only agents Pinrail starts are listed
    let mut ids: Vec<_> = agents.iter().map(|a| a["id"].as_str().unwrap()).collect();
    ids.sort();
    assert_eq!(
        ids,
        ["antigravity", "claude-code", "codex", "copilot", "grok"]
    );
    assert!(agents.iter().all(|a| a["last_used"] == false));

    // the agent started last is marked
    let work = folder(&app, "app");
    let body = json!({"host": "local", "agent": "claude-code", "folder": work});
    let (status, started) = call(&app, "POST", "/api/v1/runs", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    let (_, body) = call(&app, "GET", "/api/v1/agents/startable", None).await;
    let agents = body["agents"].as_array().unwrap();
    let last: Vec<_> = agents
        .iter()
        .filter(|a| a["last_used"] == true)
        .map(|a| &a["id"])
        .collect();
    assert_eq!(last, ["claude-code"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn recent_folders_come_most_recent_first_with_their_repository_and_branch() {
    let app = linked().await;
    let api = folder(&app, "api");
    std::fs::create_dir_all(format!("{api}/.git")).unwrap();
    std::fs::write(format!("{api}/.git/HEAD"), "ref: refs/heads/fix-login\n").unwrap();
    let web = folder(&app, "web");
    let gone = folder(&app, "gone");

    for (session, folder, repo) in [("s-gone", &gone, "acme/old"), ("s-api", &api, "acme/api")] {
        let body = json!({"kind": "step", "text": "working",
                          "origin": {"repo": repo, "folder": folder}});
        let (status, answer) = call(
            &app,
            "POST",
            &format!("/api/v1/sessions/claude-code/{session}/status"),
            Some(body),
        )
        .await;
        assert!(status.is_success(), "{answer}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // a run in a folder no session has reported yet, started last
    let (status, started) = call(
        &app,
        "POST",
        "/api/v1/runs",
        Some(json!({"agent": "claude-code", "folder": web})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    std::fs::remove_dir_all(&gone).unwrap();

    let (status, body) = call(&app, "GET", "/api/v1/folders/recent", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let folders = body["folders"].as_array().unwrap();
    let names: Vec<&str> = folders
        .iter()
        .map(|f| f["folder"].as_str().unwrap())
        .collect();
    assert_eq!(names, [web.as_str(), api.as_str(), gone.as_str()]);
    assert_eq!(folders[1]["repo"], "acme/api");
    assert_eq!(folders[1]["branch"], "fix-login");
    assert_eq!(folders[1]["exists"], true);
    assert_eq!(folders[0]["repo"], Value::Null);
    assert_eq!(folders[2]["exists"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_agents_list_is_kept_and_checked_again_only_when_due() {
    let hosts = tempfile::tempdir().unwrap();
    let registration = common::host(hosts.path());
    // the stand-in Claude Code counts each time its version is read
    let bin = hosts.path().join("counting");
    let checks = hosts.path().join("checks");
    // it counts beside the folder it is run from, so the script stays the
    // same for every run of the test
    common::program(
        &bin,
        "claude",
        "echo x >> \"$(dirname \"$0\")/../checks\"\necho '2.1.325 (Claude Code)'",
    );
    let app = common::app_with(|config| {
        config.host_socket = Some((registration.socket.clone(), registration.secret.clone()));
        config
            .run_env
            .insert("PATH".into(), format!("{}:/usr/bin:/bin", bin.display()));
    });
    runtime::watch(app.state.clone());
    app.state
        .settings()
        .change(&json!({"labs": {"runtime": true}}))
        .unwrap();
    let counted = || {
        std::fs::read_to_string(&checks)
            .map(|t| t.lines().count())
            .unwrap_or(0)
    };
    let becomes = |n: usize| {
        let deadline = Instant::now() + Duration::from_secs(10);
        async move {
            while counted() != n {
                assert!(
                    Instant::now() < deadline,
                    "checked {} times, not {n}",
                    counted()
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    };
    // the core checks once the host connects
    becomes(1).await;

    let (status, body) = call(&app, "GET", "/api/v1/agents/startable", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["checked_at"].is_string(), "{body}");
    let (_, again) = call(&app, "GET", "/api/v1/agents/startable", None).await;
    assert_eq!(again["checked_at"], body["checked_at"]);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(counted(), 1, "answered from what the host found");

    // a start that finds an agent missing makes the next request check
    let work = folder(&app, "app");
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/runs",
        Some(json!({"agent": "codex", "folder": work})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&app, "GET", "/api/v1/agents/startable", None).await;
    assert_eq!(status, StatusCode::OK);
    becomes(2).await;
    let second = checked_after(&app, &body["checked_at"]).await;

    // and so does a request that asks for it
    let (_, _) = call(&app, "GET", "/api/v1/agents/startable?refresh=true", None).await;
    becomes(3).await;
    checked_after(&app, &second).await;
    assert_eq!(counted(), 3);
}

/// When the agents were checked, once a check after `before` finished.
async fn checked_after(app: &App, before: &Value) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let (_, now) = call(app, "GET", "/api/v1/agents/startable", None).await;
        if &now["checked_at"] != before {
            return now["checked_at"].clone();
        }
        assert!(Instant::now() < deadline, "the check did not finish");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_keeps_what_it_was_started_with_and_its_last_output() {
    let app = linked().await;
    let work = folder(&app, "fails");
    // the stand-in agent exits with this code
    std::fs::write(format!("{work}/exit"), "3").unwrap();
    let body = json!({"agent": "claude-code", "folder": work, "task": "fix the login",
                      "args": ["--model", "opus"]});
    let (status, started) = call(&app, "POST", "/api/v1/runs", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    let id = started["run"]["id"].as_str().unwrap().to_string();
    let run = run_becomes(&app, &id, |run| run["state"] == "ended").await;
    assert_eq!(run["exit_code"], 3, "{run}");
    assert_eq!(run["task"], "fix the login");
    assert_eq!(run["args"], json!(["--model", "opus"]));

    // only the person reads a terminal
    let path = format!("/api/v1/runs/{id}/output?lines=20");
    let (status, _) = call_as(&app, "GET", &path, None, &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, output) = call(&app, "GET", &path, None).await;
    assert_eq!(status, StatusCode::OK, "{output}");
    assert_eq!(output["text"], "stand-in agent starting");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ended_run_gives_its_screen_as_a_paint_with_its_size() {
    let app = linked().await;
    let work = folder(&app, "ends");
    std::fs::write(format!("{work}/exit"), "0").unwrap();
    let body = json!({"agent": "claude-code", "folder": work, "cols": 132, "rows": 40});
    let (status, started) = call(&app, "POST", "/api/v1/runs", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    let id = started["run"]["id"].as_str().unwrap().to_string();
    run_becomes(&app, &id, |run| run["state"] == "ended").await;

    let path = format!("/api/v1/runs/{id}/screen");
    let (status, _) = call_as(&app, "GET", &path, None, &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, screen) = call(&app, "GET", &path, None).await;
    assert_eq!(status, StatusCode::OK, "{screen}");
    assert!(
        screen["paint"]
            .as_str()
            .unwrap()
            .contains("stand-in agent starting"),
        "{screen}"
    );
    assert_eq!(
        (screen["cols"].as_u64(), screen["rows"].as_u64()),
        (Some(132), Some(40))
    );
}

/// Starts the stand-in `claude` in `folder` and waits until it exited on
/// its own, as a stopped session. Returns the session's id.
async fn stopped_session(app: &App, folder: &str) -> String {
    std::fs::write(std::path::Path::new(folder).join("exit"), "0").unwrap();
    let (status, started) = call(
        app,
        "POST",
        "/api/v1/runs",
        Some(json!({"agent": "claude-code", "folder": folder})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    run_becomes(app, started["run"]["id"].as_str().unwrap(), |r| {
        r["state"] == "ended"
    })
    .await;
    std::fs::remove_file(std::path::Path::new(folder).join("exit")).unwrap();
    std::fs::remove_file(std::path::Path::new(folder).join("argv")).unwrap();
    started["session"]["id"].as_str().unwrap().to_string()
}

async fn resume(app: &App, agent: &str, session: &str) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        &format!("/api/v1/sessions/{agent}/{session}/resume"),
        Some(json!({"cols": 80, "rows": 24})),
    )
    .await
}

async fn argv(folder: &str) -> String {
    let path = std::path::Path::new(folder).join("argv");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(text) = std::fs::read_to_string(&path)
            && !text.is_empty()
        {
            return text;
        }
        assert!(Instant::now() < deadline, "the agent wrote no argv");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_person_resumes_a_stopped_session_in_its_folder() {
    let app = linked().await;
    let folder = folder(&app, "app");
    let session = stopped_session(&app, &folder).await;

    let (status, resumed) = resume(&app, "claude-code", &session).await;
    assert_eq!(status, StatusCode::CREATED, "{resumed}");
    let run = &resumed["run"];
    assert_eq!(run["kind"], "resume");
    assert_eq!(run["folder"], folder.as_str());
    assert_eq!(run["session"]["id"], session.as_str());
    // the agent resumes it by its own id
    let own = run["session"]["agent_session"].as_str().unwrap();
    assert_eq!(argv(&folder).await, format!("--resume\n{own}\n"));

    // a running session is not resumed twice
    let (status, again) = resume(&app, "claude-code", &session).await;
    assert_eq!(status, StatusCode::CONFLICT, "{again}");
    assert_eq!(again["error"], "already_running");
    assert_eq!(again["run"]["id"], run["id"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_resumes_at_once_start_one_run() {
    let app = linked().await;
    let folder = folder(&app, "app");
    let session = stopped_session(&app, &folder).await;

    let (first, second) = tokio::join!(
        resume(&app, "claude-code", &session),
        resume(&app, "claude-code", &session)
    );
    // the second either joins the first while it starts, or finds it running
    for (status, answer) in [&first, &second] {
        assert!(
            *status == StatusCode::CREATED || *status == StatusCode::CONFLICT,
            "{answer}"
        );
    }
    assert_eq!(first.1["run"]["id"], second.1["run"]["id"]);
    let (_, runs) = call(
        &app,
        "GET",
        &format!("/api/v1/runs?session=claude-code/{session}"),
        None,
    )
    .await;
    let resumes = runs["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "resume")
        .count();
    assert_eq!(resumes, 1, "{runs}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_resumed_agent_that_fails_at_once_ends_as_resume_failed() {
    let app = linked().await;
    let folder = folder(&app, "app");
    let session = stopped_session(&app, &folder).await;
    std::fs::write(std::path::Path::new(&folder).join("exit"), "1").unwrap();

    let (status, resumed) = resume(&app, "claude-code", &session).await;
    assert_eq!(status, StatusCode::CREATED, "{resumed}");
    let run = run_becomes(&app, resumed["run"]["id"].as_str().unwrap(), |r| {
        r["state"] == "ended"
    })
    .await;
    assert_eq!(run["end_reason"], "resume_failed", "{run}");
    assert_eq!(run["exit_code"], 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_resume_that_cannot_happen_answers_with_its_reason() {
    let app = linked().await;

    // the folder is gone
    let gone = folder(&app, "gone");
    let session = stopped_session(&app, &gone).await;
    std::fs::remove_dir_all(&gone).unwrap();
    let (status, answer) = resume(&app, "claude-code", &session).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    assert_eq!(answer["error"], "folder_missing");
    assert!(
        answer["message"].as_str().unwrap().contains(&gone),
        "{answer}"
    );

    // a session Pinrail never heard of
    let (status, _) = resume(&app, "claude-code", "no-such-session").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // an agent Pinrail cannot resume
    let here = folder(&app, "here");
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sessions/gemini/g-1/reports",
        Some(
            json!({"event": "started", "at": chrono::Utc::now().to_rfc3339(),
                    "origin": {"folder": here}}),
        ),
    )
    .await;
    assert!(status.is_success(), "{status}");
    let gemini = common::sid(&app, "gemini", "g-1");
    let (status, answer) = resume(&app, "gemini", &gemini).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    assert_eq!(answer["error"], "not_resumable");

    // the session runs in a terminal outside Pinrail: this test's process
    let pid = std::process::id();
    let started = pinrail_runtime::process::process(pid).unwrap().started;
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/c-outside/reports",
        Some(
            json!({"event": "prompt", "at": chrono::Utc::now().to_rfc3339(),
                    "process": {"pid": pid, "started": started},
                    "origin": {"folder": here}}),
        ),
    )
    .await;
    assert!(status.is_success(), "{status}");
    let outside = common::sid(&app, "claude-code", "c-outside");
    let (status, answer) = resume(&app, "claude-code", &outside).await;
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert_eq!(answer["error"], "already_running");
    assert_eq!(
        answer["message"],
        "This session is already running in another terminal."
    );
    assert!(answer["run"].is_null(), "{answer}");
    // the answer names the process that holds it, and whether the agent can
    // be resumed while it runs there
    assert_eq!(answer["process"]["pid"], pid, "{answer}");
    assert_eq!(answer["anyway"], true, "{answer}");
}

async fn resume_anyway(app: &App, agent: &str, session: &str) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        &format!("/api/v1/sessions/{agent}/{session}/resume"),
        Some(json!({"cols": 80, "rows": 24, "anyway": true})),
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_running_in_another_terminal_is_resumed_when_asked_to_anyway() {
    let app = linked().await;
    let folder = folder(&app, "outside");
    // the session runs in a terminal outside Pinrail: this test's process
    let pid = std::process::id();
    let started = pinrail_runtime::process::process(pid).unwrap().started;
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/c-outside/reports",
        Some(
            json!({"event": "prompt", "at": chrono::Utc::now().to_rfc3339(),
                    "process": {"pid": pid, "started": started},
                    "origin": {"folder": folder}}),
        ),
    )
    .await;
    assert!(status.is_success(), "{status}");

    let outside = common::sid(&app, "claude-code", "c-outside");
    let (status, resumed) = resume_anyway(&app, "claude-code", &outside).await;
    assert_eq!(status, StatusCode::CREATED, "{resumed}");
    assert_eq!(resumed["run"]["kind"], "resume");
    assert_eq!(argv(&folder).await, "--resume\nc-outside\n");

    // a session Pinrail runs is never resumed twice, anyway or not
    let (status, again) = resume_anyway(&app, "claude-code", &outside).await;
    assert_eq!(status, StatusCode::CONFLICT, "{again}");
    assert_eq!(again["run"]["id"], resumed["run"]["id"]);
    assert_eq!(again["anyway"], false, "{again}");

    // Codex is not yet known to share a session between two processes
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sessions/codex/x-outside/reports",
        Some(
            json!({"event": "prompt", "at": chrono::Utc::now().to_rfc3339(),
                    "process": {"pid": pid, "started": started},
                    "origin": {"folder": folder}}),
        ),
    )
    .await;
    assert!(status.is_success(), "{status}");
    let x_outside = common::sid(&app, "codex", "x-outside");
    let (status, refused) = resume_anyway(&app, "codex", &x_outside).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(refused["anyway"], false, "{refused}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_agents_a_restart_stopped_are_listed_until_dismissed() {
    let app = common::app();
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-rebooted/status",
        Some(json!({"kind": "start", "text": "Fix the login redirect"})),
    )
    .await;
    assert!(status.is_success());
    let db = common::db(&app);
    let session = common::sid(&app, "claude-code", "s-rebooted");
    let now = chrono::Utc::now();
    db.insert_run(
        &pinrail_core::db::NewRun {
            id: "run_rebooted",
            host: "local",
            agent: "claude-code",
            kind: "start",
            folder: "/work",
            argv: &[],
            task: None,
            args: &[],
            created_by: "app",
            token_digest: "d",
            session: Some(&session),
            agent_session: None,
            checkout: None,
        },
        now,
    )
    .unwrap();
    db.end_run("run_rebooted", "restart", None, None, now)
        .unwrap();

    let (status, listed) = call(&app, "GET", "/api/v1/runtime/restarted", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["runs"][0]["id"], "run_rebooted");
    assert_eq!(listed["runs"][0]["title"], "Fix the login redirect");
    assert_eq!(listed["runs"][0]["session"]["id"], session.as_str());
    assert_eq!(listed["runs"][0]["session"]["agent_session"], "s-rebooted");

    // the setting turns the offer off
    app.state
        .settings()
        .change(&json!({"runtime": {"after_restart": "off"}}))
        .unwrap();
    let (_, listed) = call(&app, "GET", "/api/v1/runtime/restarted", None).await;
    assert_eq!(listed["runs"], json!([]));
    app.state
        .settings()
        .change(&json!({"runtime": {"after_restart": "ask"}}))
        .unwrap();

    // only the person dismisses them
    let (status, _) = call_as(&app, "DELETE", "/api/v1/runtime/restarted", None, &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call(&app, "DELETE", "/api/v1/runtime/restarted", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, listed) = call(&app, "GET", "/api/v1/runtime/restarted", None).await;
    assert_eq!(listed["runs"], json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_that_moves_to_a_new_session_stops_the_one_it_left() {
    use pinrail_core::db::{Report, ReportEvent, Source};
    let app = linked().await;
    let folder = folder(&app, "app");
    let (_, started) = call(
        &app,
        "POST",
        "/api/v1/runs",
        Some(json!({"agent": "claude-code", "folder": folder})),
    )
    .await;
    let id = started["run"]["id"].as_str().unwrap().to_string();
    let first = started["session"]["id"].as_str().unwrap().to_string();
    let token = token(&folder).await;
    let own = common::db(&app).run(&id).unwrap().unwrap().process.unwrap();
    // its state comes from its terminal, as agy's does, and it works
    let now = chrono::Utc::now();
    let working = Report {
        event: ReportEvent::Working,
        at: now,
        process: Some(own.clone()),
        origin: Default::default(),
        transcript_path: None,
        agent_turn: None,
        cut: false,
        source: Source::Terminal,
    };
    app.state
        .reviews()
        .report("claude-code", &first, &working, false, now)
        .unwrap();

    // a session start without a source, as agy's after /clear, moves the run
    let (status, _) = report(&app, "s-next", json!({"event": "started"}), &own, &token).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    run_becomes(&app, &id, |r| r["session"]["agent_session"] == "s-next").await;
    // and the session it left no longer works
    let presence = common::db(&app)
        .presence("claude-code", &first)
        .unwrap()
        .unwrap();
    assert_eq!(
        (presence.state.as_str(), presence.stop_reason.as_deref()),
        ("stopped", Some("cleared"))
    );
}
