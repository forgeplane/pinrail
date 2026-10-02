//! End-to-end tests of the binary against a small scripted HTTP server, so
//! the exit codes and the wait loop are exercised without a real pinrail.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

type Handler = Box<dyn FnMut(&str, &str, &str) -> (u16, String) + Send>;

struct MockServer {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl MockServer {
    fn start(handler: Handler) -> Self {
        Self::start_on(TcpListener::bind("127.0.0.1:0").unwrap(), handler)
    }

    fn start_on(listener: TcpListener, handler: Handler) -> Self {
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let handler = Arc::new(Mutex::new(handler));
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let handler = handler.clone();
                let seen = seen.clone();
                thread::spawn(move || serve_one(stream, handler, seen));
            }
        });
        MockServer { url, requests }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

fn serve_one(mut stream: TcpStream, handler: Arc<Mutex<Handler>>, seen: Arc<Mutex<Vec<String>>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut content_length = 0usize;
    let mut json = false;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        if header.trim().is_empty() {
            break;
        }
        if let Some(v) = header.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
        if let Some(v) = header.to_ascii_lowercase().strip_prefix("content-type:") {
            json = v.trim().starts_with("application/json");
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).unwrap();
    }
    let body = String::from_utf8_lossy(&body).to_string();
    seen.lock().unwrap().push(format!("{method} {path}"));
    // as the server does: a write that does not say it is JSON is refused, empty or not
    let (status, response) = if (method == "POST" || method == "PATCH") && !json {
        (
            415,
            r#"{"error":"unsupported_media_type","message":"not JSON","violations":[]}"#
                .to_string(),
        )
    } else {
        handler.lock().unwrap()(&method, &path, &body)
    };
    let reason = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        404 => "Not Found",
        409 => "Conflict",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        _ => "Status",
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.len(),
        response
    );
    let _ = stream.flush();
}

/// The command as a script runs it: JSON, which most tests read back. A
/// test of the markdown an agent reads gives `--markdown`, which this takes
/// out of the arguments and turns into the default output.
fn pinrail() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pinrail"));
    cmd.env_remove("PINRAIL_URL")
        .env_remove("PINRAIL_SERVER_CMD")
        .env_remove("PINRAIL_VERBOSE")
        // the agent running the tests is not the one the tests are about
        .env_remove("AI_AGENT")
        .env_remove("AGENT")
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_ENTRYPOINT")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SANDBOX")
        .env_remove("CODEX_CI")
        .env_remove("CURSOR_AGENT")
        .env_remove("GEMINI_CLI")
        .env_remove("OPENCODE")
        .env_remove("OPENCODE_PID")
        .env_remove("OPENCODE_CLIENT")
        .env("PINRAIL_JSON", "1");
    cmd.stdin(Stdio::null());
    cmd
}

fn run(server: &MockServer, args: &[&str]) -> (i32, String, String) {
    // outside any git checkout, so no origin is filled from one
    run_in(server, &std::env::temp_dir(), args)
}

fn run_in(server: &MockServer, dir: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let markdown = args.contains(&"--markdown");
    let args: Vec<&str> = args
        .iter()
        .copied()
        .filter(|a| *a != "--markdown")
        .collect();
    let mut cmd = pinrail();
    if markdown {
        cmd.env_remove("PINRAIL_JSON");
    }
    let out = cmd
        .args(&args)
        .current_dir(dir)
        .env("PINRAIL_URL", &server.url)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn review(status: &str) -> String {
    let decision = if status == "decided" {
        r#"{"decided_by":"tester","decided_at":"2026-09-11T10:00:00Z","data":{"decisions":[{"id":1,"action":"accept"}],"undecided":[]}}"#
    } else {
        "null"
    };
    format!(
        r#"{{"id":"r_1","plugin":"list","title":"t","status":"{status}","decision":{decision},"payload":{{}}}}"#
    )
}

#[test]
fn submit_prints_the_review_and_its_id_on_stderr() {
    let server = MockServer::start(Box::new(|method, path, body| {
        assert_eq!((method, path), ("POST", "/api/v1/reviews"));
        let sent: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(sent["plugin"], "list");
        assert_eq!(sent["title"], "MR !42");
        assert_eq!(sent["origin"]["repo"], "acme");
        assert_eq!(sent["origin"]["ref"], "42");
        assert_eq!(sent["payload"]["groups"], serde_json::json!([]));
        assert_eq!(sent["requested_by"], "agent");
        (201, review("pending"))
    }));
    let dir = tempdir();
    std::fs::write(dir.join("p.json"), r#"{"groups":[]}"#).unwrap();

    let (code, stdout, stderr) = run(
        &server,
        &[
            "submit",
            "list",
            "--title",
            "MR !42",
            "--origin",
            "repo=acme,ref=42",
            "--data",
            dir.join("p.json").to_str().unwrap(),
            "--requested-by",
            "agent",
            "--no-start",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.starts_with(r#"{"id":"r_1""#), "{stdout}");
    assert_eq!(stderr, "pinrail: review r_1 submitted\n");
}

#[test]
fn submit_wait_loops_through_204_then_prints_the_decision_and_writes_the_file() {
    let mut polls = 0;
    let server = MockServer::start(Box::new(move |method, path, _| match (method, path) {
        ("POST", "/api/v1/reviews") => (201, review("pending")),
        ("GET", p) if p.starts_with("/api/v1/reviews/r_1/wait") => {
            polls += 1;
            if polls < 3 {
                (204, String::new())
            } else {
                (200, review("decided"))
            }
        }
        other => panic!("unexpected {other:?}"),
    }));
    let dir = tempdir();
    let out = dir.join("mr-42.decisions.json");

    let (code, stdout, stderr) = run(
        &server,
        &[
            "submit",
            "list",
            "--title",
            "t",
            "--wait",
            "--decision-out",
            out.to_str().unwrap(),
            "--no-start",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    let envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(envelope["status"], "decided");
    let written = std::fs::read_to_string(&out).unwrap();
    assert_eq!(
        written,
        "{\n  \"decisions\": [\n    {\n      \"id\": 1,\n      \"action\": \"accept\"\n    }\n  ],\n  \"undecided\": []\n}\n"
    );
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|r| r.contains("/wait"))
            .count(),
        3
    );
}

#[test]
fn wait_exits_3_when_the_review_is_withdrawn() {
    let server = MockServer::start(Box::new(|_, _, _| (200, review("withdrawn"))));
    let (code, stdout, stderr) = run(&server, &["wait", "r_1"]);
    assert_eq!(code, 3);
    assert!(stdout.contains(r#""status":"withdrawn""#));
    assert!(stderr.contains("was withdrawn"), "{stderr}");
}

#[test]
fn wait_exits_4_on_timeout() {
    let server = MockServer::start(Box::new(|_, path, _| {
        assert!(path.starts_with("/api/v1/reviews/r_1/wait?timeout="));
        (204, String::new())
    }));
    let (code, stdout, stderr) = run(&server, &["wait", "r_1", "--timeout", "1"]);
    assert_eq!(code, 4, "{stderr}");
    assert!(stdout.is_empty());
    assert!(stderr.contains("timed out"), "{stderr}");
}

#[test]
fn wait_survives_the_server_going_away_and_coming_back() {
    // the server answers once, then drops a connection unanswered, as one
    // going down does; the CLI must retry and finish when it answers again.
    // The port stays held throughout, so nothing else can take it meanwhile.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    thread::spawn(move || {
        let answer = |stream, status, body: String| {
            let handler: Arc<Mutex<Handler>> =
                Arc::new(Mutex::new(Box::new(move |_, _, _| (status, body.clone()))));
            serve_one(stream, handler, Arc::new(Mutex::new(Vec::new())));
        };
        answer(listener.accept().unwrap().0, 204, String::new());
        drop(listener.accept().unwrap().0);
        answer(listener.accept().unwrap().0, 200, review("decided"));
    });

    let out = pinrail()
        .args(["wait", "r_1", "--timeout", "20"])
        .env("PINRAIL_URL", &url)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(
        stderr.contains("retrying until the server is back"),
        "{stderr}"
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains(r#""status":"decided""#));
}

#[test]
fn refused_requests_exit_2_with_the_body_on_stderr() {
    let server = MockServer::start(Box::new(|_, _, _| {
        (422, r#"{"error":"invalid","message":"validation failed","violations":[{"path":"/title","message":"is required"}]}"#.into())
    }));
    let (code, stdout, stderr) = run(&server, &["submit", "list", "--title", "", "--no-start"]);
    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(stderr.contains(r#""path":"/title""#), "{stderr}");

    let server = MockServer::start(Box::new(|_, _, _| {
        (
            404,
            r#"{"error":"not_found","message":"review r_x not found","violations":[]}"#.into(),
        )
    }));
    let (code, _, stderr) = run(&server, &["show", "r_x"]);
    assert_eq!(code, 2);
    assert!(stderr.contains("not_found"));
}

#[test]
fn open_refuses_a_review_the_app_does_not_have_before_opening_anything() {
    let server = MockServer::start(Box::new(|method, path, _| {
        assert_eq!((method, path), ("GET", "/api/v1/reviews/r_x"));
        (
            404,
            r#"{"error":"not_found","message":"review r_x not found","violations":[]}"#.into(),
        )
    }));
    let (code, stdout, stderr) = run(&server, &["open", "r_x", "--markdown"]);
    assert_eq!(code, 2);
    assert!(stdout.is_empty(), "{stdout}");
    assert_eq!(stderr, "pinrail: refused: review r_x not found\n");
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn a_review_is_requested_by_the_agent_the_cli_runs_under_unless_told() {
    let sent = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = sent.clone();
    let server = MockServer::start(Box::new(move |_, _, body| {
        let body: serde_json::Value = serde_json::from_str(body).unwrap();
        seen.lock().unwrap().push(
            body["requested_by"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        );
        (201, review("pending"))
    }));
    let submit = |envs: &[(&str, &str)], extra: &[&str]| {
        let mut cmd = pinrail();
        cmd.args(["submit", "list", "--title", "t", "--no-start"])
            .args(extra)
            .env("PINRAIL_URL", &server.url)
            .current_dir(std::env::temp_dir());
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    submit(&[("CLAUDECODE", "1")], &[]);
    submit(&[("AI_AGENT", "claude-code_2-1-281_agent")], &[]);
    submit(&[("CODEX_SANDBOX", "seatbelt")], &[]);
    submit(&[], &[]);
    submit(&[("CLAUDECODE", "1")], &["--requested-by", "pr-reviewer"]);
    submit(
        &[("CLAUDECODE", "1"), ("PINRAIL_REQUESTED_BY", "nightly")],
        &[],
    );
    assert_eq!(
        *sent.lock().unwrap(),
        [
            "claude-code",
            "claude-code",
            "codex",
            "pinrail-cli",
            "pr-reviewer",
            "nightly"
        ]
    );
}

#[test]
fn unknown_origin_keys_are_dropped_with_a_warning() {
    let server = MockServer::start(Box::new(|method, path, body| {
        assert_eq!((method, path), ("POST", "/api/v1/reviews"));
        let sent: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(
            sent["origin"],
            serde_json::json!({"repo": "acme", "ref": "42"})
        );
        (201, review("pending"))
    }));
    let dir = tempdir();
    std::fs::write(
        dir.join("r.json"),
        r#"{"plugin":"list","title":"t","origin":{"repo":"acme","team":"core"}}"#,
    )
    .unwrap();

    let (code, _, stderr) = run(
        &server,
        &[
            "submit",
            "list",
            "--title",
            "t",
            "--origin",
            "repo=acme,ref=42,agnet=codex",
            "--no-start",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains("pinrail: warning: origin key agnet dropped: the app keeps repo, ref, workflow, run_id, url\n"), "{stderr}");

    // from a request file too
    let (_, _, stderr) = run(
        &server,
        &[
            "submit",
            "--request",
            dir.join("r.json").to_str().unwrap(),
            "--origin",
            "repo=acme,ref=42",
            "--no-start",
        ],
    );
    assert!(
        !stderr.contains("warning"),
        "the flag replaced the file's origin: {stderr}"
    );
    let server2 = MockServer::start(Box::new(|_, _, body| {
        let sent: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(sent["origin"], serde_json::json!({"repo": "acme"}));
        (201, review("pending"))
    }));
    let (code, _, stderr) = run(
        &server2,
        &[
            "submit",
            "--request",
            dir.join("r.json").to_str().unwrap(),
            "--no-start",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains("origin key team dropped"), "{stderr}");
}

#[test]
fn a_refused_payload_reads_as_markdown_and_points_at_the_plugins_shape() {
    let server = MockServer::start(Box::new(|_, _, _| {
        (422, r#"{"error":"invalid","message":"validation failed","violations":[{"path":"/payload/groups","message":"value is not of type array"}]}"#.into())
    }));
    let (code, stdout, stderr) = run(
        &server,
        &["submit", "list", "--title", "t", "--no-start", "--markdown"],
    );
    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert_eq!(
        stderr,
        "pinrail: refused:\n  /payload/groups: value is not of type array\nThe payload it takes: pinrail plugins describe list\n"
    );
}

#[test]
fn a_plugin_that_is_not_installed_points_at_the_list_of_those_that_are() {
    // a guessed or misspelled name: the next step is to see what there is
    let server = MockServer::start(Box::new(|_, path, _| {
        match path {
        "/api/v1/reviews" => (422, r#"{"error":"invalid","message":"validation failed","violations":[{"path":"/plugin","message":"unknown plugin code_review"}]}"#.into()),
        "/api/v1/plugins/code_review/describe" => (404, r#"{"error":"not_found","message":"plugin code_review not found","violations":[]}"#.into()),
        "/api/v1/plugins" => (200, r#"{"plugins":[]}"#.into()),
        other => panic!("unexpected {other}"),
    }
    }));
    let (code, _, stderr) = run(
        &server,
        &[
            "submit",
            "code_review",
            "--title",
            "t",
            "--no-start",
            "--markdown",
        ],
    );
    assert_eq!(code, 2);
    assert!(
        stderr.ends_with("Installed plugins: pinrail plugins\n"),
        "{stderr}"
    );

    let (code, _, stderr) = run(
        &server,
        &["plugins", "describe", "code_review", "--markdown"],
    );
    assert_eq!(code, 2);
    assert!(
        stderr.ends_with("Installed plugins: pinrail plugins\n"),
        "{stderr}"
    );
}

#[test]
fn waiting_flags_on_a_submit_that_does_not_wait_are_refused() {
    // without --wait the command returns at once: a decision file would
    // never be written, and an agent reading it later would find nothing
    let server = MockServer::start(Box::new(|_, _, _| (201, review("pending"))));
    for flag in [["--decision-out", "d.json"], ["--timeout", "60"]] {
        let (code, _, stderr) = run(
            &server,
            &[
                "submit",
                "list",
                "--title",
                "t",
                "--no-start",
                flag[0],
                flag[1],
            ],
        );
        assert_eq!(code, 1, "{} without --wait: {stderr}", flag[0]);
        assert!(stderr.contains("--wait"), "{stderr}");
    }
}

#[test]
fn wait_with_no_server_says_so_instead_of_calling_the_review_pending() {
    // nothing listens on this port: the app is closed, or the URL is wrong
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let out = pinrail()
        .args([
            "--url",
            &format!("http://127.0.0.1:{port}"),
            "wait",
            "r_1",
            "--timeout",
            "5",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(1),
        "not a timeout on a review: {stderr}"
    );
    assert!(stderr.contains("open the Pinrail app"), "{stderr}");
    assert!(!stderr.contains("still pending"), "{stderr}");
}

#[test]
fn a_command_with_no_server_says_to_open_the_app() {
    // the first discovery commands an agent runs, with the app closed
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let url = format!("http://127.0.0.1:{port}");
    for args in [&["plugins"][..], &["list"], &["show", "r_1"]] {
        let out = pinrail()
            .arg("--url")
            .arg(&url)
            .args(args)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {stderr}");
        assert!(
            stderr.contains(&format!(
                "not answering at {url} (from --url or PINRAIL_URL); open the Pinrail app"
            )),
            "{args:?}: {stderr}"
        );
    }
}

#[test]
fn ids_and_names_stay_one_path_segment() {
    // what is typed as an id or a name must not become a query or a route
    let paths = Arc::new(Mutex::new(Vec::new()));
    let seen = paths.clone();
    let server = MockServer::start(Box::new(move |_, path, _| {
        seen.lock().unwrap().push(path.to_string());
        (
            404,
            r#"{"error":"not_found","message":"not found","violations":[]}"#.into(),
        )
    }));
    run(&server, &["show", "r_1?format=markdown"]);
    run(&server, &["plugins", "remove", "a/b"]);
    let paths = paths.lock().unwrap();
    assert!(
        paths.contains(&"/api/v1/reviews/r_1%3Fformat%3Dmarkdown".to_string()),
        "{paths:?}"
    );
    assert!(
        paths.contains(&"/api/v1/plugins/a%2Fb".to_string()),
        "{paths:?}"
    );
}

#[test]
fn a_decision_that_cannot_be_rendered_still_exits_as_decided() {
    // the review ended and its decision arrived; only the markdown failed
    let server = MockServer::start(Box::new(|_, path, _| {
        if path.starts_with("/api/v1/reviews/r_1/wait") {
            (200, review("decided"))
        } else {
            (
                500,
                r#"{"error":"internal","message":"the template broke","violations":[]}"#.into(),
            )
        }
    }));
    let (code, stdout, stderr) = run(&server, &["wait", "r_1", "--markdown"]);
    assert_eq!(code, 0, "decided is decided: {stderr}");
    assert!(stdout.contains("decided"), "{stdout}");
    assert!(
        stdout.contains("\"action\": \"accept\""),
        "the decision itself: {stdout}"
    );
    assert!(stderr.contains("markdown"), "{stderr}");
}

/// Escape sequences, and a right-to-left override that reorders what follows it.
const HOSTILE: &str = "\u{1b}[2J\u{1b}]52;c;cm0gLXJm\u{7}\u{202e}";

fn assert_harmless(text: &str) {
    assert!(
        !text.contains('\u{1b}') && !text.contains('\u{7}'),
        "{text:?}"
    );
    assert!(!text.contains('\u{202e}'), "{text:?}");
}

#[test]
fn a_plugin_description_cannot_drive_the_terminal() {
    let listing = serde_json::json!({ "plugins": [{
        "name": "hello", "title": "Hello", "version": 1, "release": "1.0.0", "usable": true,
        "description": format!("Greets{HOSTILE} people"), "use_when": format!("always{HOSTILE}"),
    }]});
    let server = MockServer::start(Box::new(move |_, path, _| match path {
        "/api/v1/plugins" => (200, listing.to_string()),
        other => panic!("unexpected {other}"),
    }));
    let (code, stdout, stderr) = run(&server, &["plugins", "--markdown"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("Greets"), "{stdout}");
    assert_harmless(&stdout);
}

#[test]
fn a_refusal_cannot_drive_the_terminal() {
    let body = serde_json::json!({ "error": "invalid", "message": "validation failed",
        "violations": [{ "path": "/payload", "message": format!("bad{HOSTILE} value") }] });
    let server = MockServer::start(Box::new(move |_, _, _| (422, body.to_string())));
    let (code, _, stderr) = run(
        &server,
        &["submit", "list", "--title", "t", "--no-start", "--markdown"],
    );
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("bad"), "{stderr}");
    assert_harmless(&stderr);
}

#[test]
fn a_discard_reason_cannot_drive_the_terminal() {
    let review = serde_json::json!({ "id": "r_1", "plugin": "list", "title": "t", "status": "discarded",
        "decision": null, "payload": {}, "discarded_by": "pat", "discarded_reason": format!("no{HOSTILE} thanks") });
    let server = MockServer::start(Box::new(move |_, _, _| (200, review.to_string())));
    let (code, _, stderr) = run(&server, &["wait", "r_1", "--json"]);
    assert_eq!(code, 5, "{stderr}");
    assert!(stderr.contains("discarded by pat"), "{stderr}");
    assert_harmless(&stderr);
}

#[test]
fn updating_every_plugin_exits_1_when_the_app_fails_and_2_when_it_refuses() {
    let listing = r#"{"plugins":[{"name":"one","install":{"linked":false}},{"name":"two","install":{"linked":false}}]}"#;
    // one refused (a 4xx), the other failing on the app's side (a 5xx)
    let failing = |second: u16| {
        MockServer::start(Box::new(move |method, path, _| match (method, path) {
            ("GET", "/api/v1/plugins") => (200, listing.into()),
            ("POST", "/api/v1/plugins/one/update/inspect") => (
                422,
                r#"{"error":"invalid","message":"pinned","violations":[]}"#.into(),
            ),
            ("POST", "/api/v1/plugins/two/update/inspect") => (
                second,
                r#"{"error":"internal","message":"git failed","violations":[]}"#.into(),
            ),
            other => panic!("unexpected {other:?}"),
        }))
    };
    let (code, _, stderr) = run(&failing(500), &["plugins", "update", "--json"]);
    assert_eq!(
        code, 1,
        "a failure on the app's side is not a refusal: {stderr}"
    );
    let (code, _, stderr) = run(&failing(409), &["plugins", "update", "--json"]);
    assert_eq!(code, 2, "only refusals: {stderr}");
}

#[test]
fn a_slow_answer_is_not_reported_as_an_app_that_is_not_running() {
    // the app is there and takes its time: past the CLI's 15 seconds
    let server = MockServer::start(Box::new(|_, _, _| {
        std::thread::sleep(std::time::Duration::from_secs(17));
        (200, review("pending"))
    }));
    let (code, _, stderr) = run(&server, &["show", "r_1"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("did not finish answering"), "{stderr}");
    assert!(!stderr.contains("open the Pinrail app"), "{stderr}");
}

#[test]
fn a_wait_that_runs_out_gives_the_command_to_keep_waiting() {
    let server = MockServer::start(Box::new(|_, path, _| {
        assert!(path.starts_with("/api/v1/reviews/r_1/wait"), "{path}");
        (204, String::new())
    }));
    let (code, _, stderr) = run(&server, &["wait", "r_1", "--timeout", "1"]);
    assert_eq!(code, 4, "{stderr}");
    assert!(stderr.contains("pinrail wait r_1 --timeout 1"), "{stderr}");

    // the limit set once, in the environment
    let out = pinrail()
        .args(["--url", &server.url, "wait", "r_1"])
        .env("PINRAIL_TIMEOUT", "1")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn json_can_be_given_inline_as_well_as_in_a_file() {
    let bodies = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let seen = bodies.clone();
    let server = MockServer::start(Box::new(move |method, path, body| {
        if method == "POST" {
            seen.lock()
                .unwrap()
                .push(serde_json::from_str(body).unwrap());
        }
        match path {
            "/api/v1/reviews" => (201, review("pending")),
            "/api/v1/reviews/r_1/decision" => (200, review("decided")),
            other => panic!("unexpected {other}"),
        }
    }));
    let (code, _, stderr) = run(
        &server,
        &[
            "submit",
            "list",
            "--title",
            "t",
            "--data",
            r#"{"groups": []}"#,
            "--no-start",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    let (code, _, stderr) = run(
        &server,
        &[
            "submit",
            "--request",
            r#"{"plugin": "list", "title": "whole", "payload": {"groups": []}}"#,
            "--no-start",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    let (code, _, stderr) = run(
        &server,
        &["decide", "r_1", "--data", r#"{"decisions": []}"#],
    );
    assert_eq!(code, 0, "{stderr}");
    let bodies = bodies.lock().unwrap();
    assert_eq!(bodies[0]["payload"], serde_json::json!({ "groups": [] }));
    assert_eq!(bodies[1]["title"], "whole");
    assert_eq!(bodies[2]["data"], serde_json::json!({ "decisions": [] }));

    // JSON that does not parse says so, rather than naming a missing file
    let (code, _, stderr) = run(
        &server,
        &[
            "submit",
            "list",
            "--title",
            "t",
            "--data",
            "{groups: []}",
            "--no-start",
        ],
    );
    assert_eq!(code, 1);
    assert!(stderr.contains("not valid JSON"), "{stderr}");
    assert!(!stderr.contains("No such file"), "{stderr}");
}

#[test]
fn every_command_help_names_the_global_options() {
    for command in [
        &["list"][..],
        &["show"],
        &["wait"],
        &["submit"],
        &["plugins"],
        &["plugins", "describe"],
    ] {
        let out = pinrail().args(command).arg("--help").output().unwrap();
        let help = String::from_utf8_lossy(&out.stdout);
        for option in ["--json", "--url"] {
            assert!(
                help.contains(option),
                "{command:?} --help leaves out {option}:\n{help}"
            );
        }
    }
}

#[test]
fn with_json_an_error_on_stderr_is_one_line_of_json() {
    let server = MockServer::start(Box::new(|_, _, _| {
        (422, r#"{"error":"invalid","message":"validation failed","violations":[{"path":"/title","message":"is required"}]}"#.into())
    }));
    let (code, _, stderr) = run(&server, &["submit", "list", "--title", "", "--no-start"]);
    assert_eq!(code, 2);
    let error: serde_json::Value = serde_json::from_str(stderr.trim()).expect(&stderr);
    assert_eq!(error["error"], "invalid");
    assert_eq!(error["violations"][0]["path"], "/title");
    assert_eq!(error["exit"], 2);

    // an error of the CLI's own, with nothing listening
    let out = pinrail()
        .args(["--url", "http://127.0.0.1:1", "show", "r_1"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1));
    let error: serde_json::Value = serde_json::from_str(stderr.trim()).expect(&stderr);
    assert_eq!(error["error"], "cli");
    assert_eq!(error["exit"], 1);
    assert!(
        error["message"].as_str().unwrap().contains("not answering"),
        "{error}"
    );
}

#[test]
fn plugin_versions_say_when_no_version_is_usable() {
    let server = MockServer::start(Box::new(|_, path, _| match path {
        "/api/v1/plugins/hello/versions" => (
            200,
            r#"{"name":"hello","current":null,"versions":[1,2]}"#.into(),
        ),
        "/api/v1/plugins/list/versions" => (
            200,
            r#"{"name":"list","current":2,"versions":[1,2]}"#.into(),
        ),
        other => panic!("unexpected {other}"),
    }));
    let (code, stdout, stderr) = run(&server, &["plugins", "versions", "hello", "--markdown"]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        stdout,
        "hello: no usable version installed; reviews render with v1, v2.\n"
    );
    let (_, stdout, _) = run(&server, &["plugins", "versions", "list", "--markdown"]);
    assert_eq!(stdout, "list: current v2; reviews render with v1, v2.\n");
}

#[test]
fn text_from_a_review_cannot_drive_the_terminal() {
    // a title with escape sequences: clear the screen, set the clipboard
    let server = MockServer::start(Box::new(|_, path, _| {
        if path.starts_with("/api/v1/reviews/r_1/wait") {
            (200, review("decided"))
        } else {
            (
                200,
                "r_1 · decided · Ship\u{1b}[2J it\u{1b}]52;c;cm0gLXJm\u{7}\nlist\n\n\t- a tab stays\n"
                    .into(),
            )
        }
    }));
    let (code, stdout, stderr) = run(&server, &["wait", "r_1", "--markdown"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        !stdout.contains('\u{1b}') && !stdout.contains('\u{7}'),
        "{stdout:?}"
    );
    assert!(
        stdout.contains("Ship it\nlist\n\n\t- a tab stays"),
        "{stdout:?}"
    );
}

/// A server error is not a refusal: there is nothing for the agent to fix
/// and resubmit, so it exits 1, and a wait keeps waiting through one.
#[test]
fn a_server_error_is_not_a_refusal_and_a_wait_rides_it_out() {
    let server = MockServer::start(Box::new(|_, _, _| {
        (
            500,
            r#"{"error":"internal","message":"database: disk I/O error","violations":[]}"#.into(),
        )
    }));
    let (code, _, stderr) = run(&server, &["show", "r_1"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("disk I/O error"), "{stderr}");

    let polls = Arc::new(Mutex::new(0));
    let seen = polls.clone();
    let server = MockServer::start(Box::new(move |_, path, _| {
        if !path.starts_with("/api/v1/reviews/r_1/wait") {
            return (
                404,
                r#"{"error":"not_found","message":"no","violations":[]}"#.into(),
            );
        }
        let mut n = seen.lock().unwrap();
        *n += 1;
        if *n == 1 {
            (
                500,
                r#"{"error":"internal","message":"database is locked","violations":[]}"#.into(),
            )
        } else {
            (200, review("decided"))
        }
    }));
    let (code, stdout, stderr) = run(&server, &["wait", "r_1"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("\"decided\""), "{stdout}");
    assert!(stderr.contains("database is locked"), "{stderr}");
    assert!(*polls.lock().unwrap() >= 2);
}

const INSPECTED_WITHOUT_BUILD: &str = r#"{"name":"triage","version":"1.0.0","build":null,"expect":{"build":null,"commit":"abc1234"}}"#;
const INSPECTED_WITH_BUILD: &str = r#"{"state":"available","name":"triage","version":"1.1.0","build":"npm ci && npm run build","expect":{"build":"npm ci && npm run build","commit":"def5678"}}"#;
const JOB_DONE: &str =
    r#"{"status":"done","log":"","log_offset":0,"plugin":{"name":"triage","release":"1.1.0"}}"#;

/// A server that answers inspections with `inspected`, and records the
/// body of every install or update it is asked to start.
fn plugin_server(inspected: &'static str) -> (MockServer, Arc<Mutex<Vec<String>>>) {
    let started = Arc::new(Mutex::new(Vec::new()));
    let seen = started.clone();
    let server = MockServer::start(Box::new(move |method, path, body| match (method, path) {
        ("GET", "/api/v1/plugins") => (
            200,
            r#"{"plugins":[{"name":"triage","install":{"linked":false}}]}"#.into(),
        ),
        ("POST", "/api/v1/plugins/inspect" | "/api/v1/plugins/triage/update/inspect") => {
            (200, inspected.into())
        }
        ("POST", "/api/v1/plugins/install" | "/api/v1/plugins/triage/update") => {
            seen.lock().unwrap().push(body.to_string());
            (202, r#"{"job":"j_1"}"#.into())
        }
        ("GET", "/api/v1/plugins/jobs/j_1") => (200, JOB_DONE.into()),
        other => panic!("unexpected {other:?}"),
    }));
    (server, started)
}

#[test]
fn a_build_runs_only_with_yes_when_nobody_is_at_a_terminal_to_confirm_it() {
    for args in [
        &["plugins", "install", "github.com/acme/triage"][..],
        &["plugins", "update", "triage"],
    ] {
        let (server, started) = plugin_server(INSPECTED_WITH_BUILD);
        let (code, _, stderr) = run(&server, args);
        assert_eq!(code, 2, "{args:?}: {stderr}");
        assert!(stderr.contains("npm ci && npm run build"), "{stderr}");
        assert!(stderr.contains("--yes"), "{stderr}");
        assert!(
            started.lock().unwrap().is_empty(),
            "{args:?} started without a yes"
        );

        let with_yes = [args, &["--yes"]].concat();
        let (code, _, stderr) = run(&server, &with_yes);
        assert_eq!(code, 0, "{with_yes:?}: {stderr}");
        let body: serde_json::Value = serde_json::from_str(&started.lock().unwrap()[0]).unwrap();
        assert_eq!(
            body["expect"],
            serde_json::json!({"build": "npm ci && npm run build", "commit": "def5678"}),
            "{with_yes:?}"
        );
    }
}

#[test]
fn a_plugin_without_a_build_installs_as_it_was_inspected() {
    let (server, started) = plugin_server(INSPECTED_WITHOUT_BUILD);
    let (code, _, stderr) = run(&server, &["plugins", "install", "github.com/acme/triage"]);
    assert_eq!(code, 0, "{stderr}");
    let body: serde_json::Value = serde_json::from_str(&started.lock().unwrap()[0]).unwrap();
    assert_eq!(
        body["expect"],
        serde_json::json!({"build": null, "commit": "abc1234"})
    );
}

#[test]
fn updating_every_plugin_leaves_out_a_build_nobody_confirmed() {
    let (server, started) = plugin_server(INSPECTED_WITH_BUILD);
    let (code, stdout, stderr) = run(&server, &["plugins", "update"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(started.lock().unwrap().is_empty());
    let answers: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(answers[0]["state"], "failed", "{answers}");
    assert!(
        answers[0]["error"].as_str().unwrap().contains("--yes"),
        "{answers}"
    );
}

#[test]
fn an_install_that_cannot_reach_its_source_exits_1_and_a_wrong_source_2() {
    for (kind, exit) in [("unavailable", 1), ("invalid", 2)] {
        let failed = format!(
            r#"{{"status":"failed","log":"","log_offset":0,"error":"git clone failed","error_kind":"{kind}","plugin":null}}"#
        );
        let server = MockServer::start(Box::new(move |_, path, _| match path {
            "/api/v1/plugins/inspect" => (200, INSPECTED_WITHOUT_BUILD.into()),
            "/api/v1/plugins/install" => (202, r#"{"job":"j_1"}"#.into()),
            "/api/v1/plugins/jobs/j_1" => (200, failed.clone()),
            other => panic!("unexpected {other}"),
        }));
        let (code, _, stderr) = run(&server, &["plugins", "install", "github.com/acme/triage"]);
        assert_eq!(code, exit, "{kind}: {stderr}");
    }
}

#[test]
fn an_install_answer_with_no_job_says_so() {
    let server = MockServer::start(Box::new(|_, path, _| match path {
        "/api/v1/plugins/inspect" => (200, INSPECTED_WITHOUT_BUILD.into()),
        "/api/v1/plugins/install" => (202, "{}".into()),
        other => (
            404,
            format!(r#"{{"error":"not_found","message":"{other} not found","violations":[]}}"#),
        ),
    }));
    let (code, _, stderr) = run(&server, &["plugins", "install", "github.com/acme/triage"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("started no install job"), "{stderr}");
}

#[test]
fn a_status_the_cli_does_not_know_is_an_error_not_an_ending() {
    // a newer server, or a body that is not a review: exit 3 would tell the
    // agent that nobody decided
    let server = MockServer::start(Box::new(|_, _, _| {
        (200, r#"{"id":"r_1","plugin":"list","title":"t","status":"archived","decision":null,"payload":{}}"#.into())
    }));
    let (code, _, stderr) = run(&server, &["wait", "r_1"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("archived"), "{stderr}");
}

#[test]
fn list_searches_with_search() {
    let server = MockServer::start(Box::new(|method, path, _| match (method, path) {
        ("GET", p) if p.starts_with("/api/v1/reviews?") && p.contains("q=dedup") => (
            200,
            r#"{"reviews":[],"total":0,"has_more":false,"next_cursor":null}"#.into(),
        ),
        other => panic!("unexpected {other:?}"),
    }));
    let (code, _, stderr) = run(&server, &["list", "--search", "dedup"]);
    assert_eq!(code, 0, "{stderr}");
    let (_, help, _) = run(&server, &["list", "--help"]);
    assert!(help.contains("--search <WORDS>"), "{help}");
}

#[test]
fn list_all_follows_the_cursor_to_the_last_page() {
    let server = MockServer::start(Box::new(|method, path, _| {
        match (method, path) {
        ("GET", "/api/v1/reviews?status=decided") => (
            200,
            r#"{"reviews":[{"id":"r_4"},{"id":"r_3"}],"total":5,"has_more":true,"next_cursor":"r_3"}"#.into(),
        ),
        ("GET", "/api/v1/reviews?status=decided&cursor=r_3") => (
            200,
            r#"{"reviews":[{"id":"r_2"},{"id":"r_1"}],"total":5,"has_more":true,"next_cursor":"r_1"}"#.into(),
        ),
        ("GET", "/api/v1/reviews?status=decided&cursor=r_1") => (
            200,
            r#"{"reviews":[{"id":"r_0"}],"total":5,"has_more":false,"next_cursor":null}"#.into(),
        ),
        other => panic!("unexpected {other:?}"),
    }
    }));
    let (code, stdout, stderr) = run(&server, &["list", "--status", "decided", "--all"]);
    assert_eq!(code, 0, "{stderr}");
    let ids: Vec<String> = serde_json::from_str::<serde_json::Value>(&stdout)
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, vec!["r_4", "r_3", "r_2", "r_1", "r_0"]);
    assert_eq!(server.requests().len(), 3);

    // without --all, one page, as a plain array
    let (code, stdout, _) = run(&server, &["list", "--status", "decided"]);
    assert_eq!(code, 0);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&stdout)
            .unwrap()
            .as_array()
            .map(Vec::len),
        Some(2)
    );
}

#[test]
fn list_show_withdraw_decide_and_plugins_hit_the_right_endpoints() {
    let server = MockServer::start(Box::new(|method, path, body| match (method, path) {
        ("GET", "/api/v1/reviews?status=pending&repo=acme") => (
            200,
            r#"{"reviews":[],"total":0,"has_more":false,"next_cursor":null}"#.into(),
        ),
        ("GET", "/api/v1/reviews?status=pending&include_revised=true") => (
            200,
            r#"{"reviews":[],"total":0,"has_more":false,"next_cursor":null}"#.into(),
        ),
        ("GET", "/api/v1/reviews/r_1") => (200, review("pending")),
        ("GET", "/api/v1/reviews/r_1/rounds") => (200, "[]".into()),
        ("POST", "/api/v1/reviews/r_1/withdraw") => {
            let sent: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(sent, serde_json::json!({"reason":"moved on"}));
            (200, review("withdrawn"))
        }
        ("POST", "/api/v1/reviews/r_1/decision") => {
            let sent: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(
                sent,
                serde_json::json!({"data":{"ok":true},"agent_note":"fine"})
            );
            (200, review("decided"))
        }
        ("GET", "/api/v1/plugins") => (200, r#"{"plugins":[]}"#.into()),
        ("GET", "/api/v1/plugins/list/versions") => {
            (200, r#"{"name":"list","versions":[1]}"#.into())
        }
        ("POST", "/api/v1/plugins/reload") => (200, r#"{"ok":true,"count":1}"#.into()),
        other => panic!("unexpected {other:?}"),
    }));
    let dir = tempdir();
    std::fs::write(dir.join("d.json"), r#"{"ok":true}"#).unwrap();

    assert_eq!(
        run(&server, &["list", "--status", "pending", "--repo", "acme"]).0,
        0
    );
    assert_eq!(run(&server, &["list", "--include-revised"]).0, 0);
    let (code, stdout, _) = run(&server, &["show", "r_1", "--pretty"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("\n  \"id\": \"r_1\""));
    assert_eq!(run(&server, &["rounds", "r_1"]).0, 0);
    assert_eq!(
        run(&server, &["withdraw", "r_1", "--reason", "moved on"]).0,
        0
    );
    assert_eq!(
        run(
            &server,
            &[
                "decide",
                "r_1",
                "--data",
                dir.join("d.json").to_str().unwrap(),
                "--note",
                "fine"
            ]
        )
        .0,
        0
    );
    assert_eq!(run(&server, &["plugins"]).0, 0);
    assert_eq!(run(&server, &["plugins", "reload"]).0, 0);
    assert_eq!(run(&server, &["plugins", "versions", "list"]).0, 0);
}

#[test]
fn unreachable_server_exits_1_without_auto_start_config() {
    let out = pinrail()
        .args(["show", "r_1"])
        .env("PINRAIL_URL", "http://127.0.0.1:9")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("the server is not answering at http://127.0.0.1:9 (from --url or PINRAIL_URL); open the Pinrail app")
    );

    let dir = tempdir();
    let out = pinrail()
        .args(["submit", "list", "--title", "t"])
        .env("PINRAIL_URL", "http://127.0.0.1:9")
        .env("PINRAIL_DATA_DIR", &dir)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    // an explicit --url/PINRAIL_URL is never auto-started; discovery would be
    assert!(String::from_utf8_lossy(&out.stderr).contains("not answering at http://127.0.0.1:9"));
}

#[test]
fn discovers_the_server_from_server_json_in_the_data_dir() {
    let server = MockServer::start(Box::new(|_, _, _| (200, review("pending"))));
    let dir = tempdir();
    std::fs::write(
        dir.join("server.json"),
        format!(r#"{{"url":"{}","port":1}}"#, server.url),
    )
    .unwrap();
    let out = pinrail()
        .args(["show", "r_1"])
        .env("PINRAIL_DATA_DIR", &dir)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn create_auto_starts_the_server_with_the_configured_command() {
    // the "server" is a shell one-liner that writes server.json pointing at a mock
    let server = MockServer::start(Box::new(|method, path, _| match (method, path) {
        ("GET", "/api/v1/info") => (200, "{}".into()),
        ("POST", "/api/v1/reviews") => (201, review("pending")),
        other => panic!("unexpected {other:?}"),
    }));
    let dir = tempdir();
    // a stale server.json from a previous run points at a dead port, so
    // discovery never depends on whatever happens to listen on 4747
    std::fs::write(dir.join("server.json"), r#"{"url":"http://127.0.0.1:9"}"#).unwrap();
    let cmd = format!(
        "sleep 0.3; printf '{{\"url\":\"{}\"}}' > \"$PINRAIL_DATA_DIR/server.json\"; sleep 5",
        server.url
    );
    let out = pinrail()
        .args(["submit", "list", "--title", "t", "--verbose"])
        .env("PINRAIL_DATA_DIR", &dir)
        .env("PINRAIL_SERVER_CMD", &cmd)
        .env("PINRAIL_PORT", "9")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(stderr.contains("starting it"), "{stderr}");
    assert!(dir.join("server.log").exists());
}

/// A server that another start brought up counts, even when it is not
/// the one this start ran: two agents starting the app at once end up
/// with one server, and both use it.
#[test]
fn a_start_uses_any_server_that_comes_up_at_the_address() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    // the other start's server, up a moment after this one began waiting
    let other = thread::spawn(move || {
        thread::sleep(std::time::Duration::from_millis(700));
        MockServer::start_on(
            TcpListener::bind(("127.0.0.1", port)).unwrap(),
            Box::new(|method, path, _| match (method, path) {
                ("GET", "/api/v1/info") => (200, "{}".into()),
                ("GET", "/api/v1/plugins") => (200, r#"{"plugins":[]}"#.into()),
                other => panic!("unexpected {other:?}"),
            }),
        )
    });
    let dir = tempdir();
    let started = std::time::Instant::now();
    let out = pinrail()
        .args(["plugins"])
        .env("PINRAIL_DATA_DIR", &dir)
        // this start's own server never comes up
        .env("PINRAIL_SERVER_CMD", "sleep 30")
        .env("PINRAIL_PORT", port.to_string())
        .output()
        .unwrap();
    let _server = other.join().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
}

/// A start command that fails is said at once, with its log, and the log
/// another server was writing is kept.
#[test]
fn a_start_command_that_exits_fails_at_once_and_keeps_the_log() {
    let dir = tempdir();
    std::fs::write(dir.join("server.log"), "an earlier server's log\n").unwrap();
    std::fs::write(dir.join("server.json"), r#"{"url":"http://127.0.0.1:9"}"#).unwrap();
    let started = std::time::Instant::now();
    let out = pinrail()
        .args(["plugins"])
        .env("PINRAIL_DATA_DIR", &dir)
        .env("PINRAIL_SERVER_CMD", "echo no such app >&2; exit 127")
        .env("PINRAIL_PORT", "9")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "{stderr}"
    );
    assert!(stderr.contains("server.log"), "{stderr}");
    let log = std::fs::read_to_string(dir.join("server.log")).unwrap();
    assert!(log.starts_with("an earlier server's log\n"), "{log}");
    assert!(log.contains("no such app"), "{log}");
    assert!(dir.join("server.json").exists());
}

#[test]
fn listing_the_plugins_starts_the_server_too() {
    let server = MockServer::start(Box::new(|method, path, _| match (method, path) {
        ("GET", "/api/v1/info") => (200, "{}".into()),
        ("GET", "/api/v1/plugins") => (200, r#"{"plugins":[]}"#.into()),
        other => panic!("unexpected {other:?}"),
    }));
    let dir = tempdir();
    std::fs::write(dir.join("server.json"), r#"{"url":"http://127.0.0.1:9"}"#).unwrap();
    let cmd = format!(
        "sleep 0.3; printf '{{\"url\":\"{}\"}}' > \"$PINRAIL_DATA_DIR/server.json\"; sleep 5",
        server.url
    );
    let out = pinrail()
        .args(["plugins", "--verbose"])
        .env("PINRAIL_DATA_DIR", &dir)
        .env("PINRAIL_SERVER_CMD", &cmd)
        .env("PINRAIL_PORT", "9")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(stderr.contains("starting it"), "{stderr}");
}

#[test]
fn a_server_that_does_not_answer_is_named_with_where_its_address_came_from() {
    let message = |args: &[&str], envs: &[(&str, &str)]| {
        let dir = tempdir();
        let mut cmd = pinrail();
        cmd.args(args).env("PINRAIL_DATA_DIR", &dir);
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let out = cmd.output().unwrap();
        assert_eq!(out.status.code(), Some(1));
        let error: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
        error["message"].as_str().unwrap().to_string()
    };
    let given = message(&["show", "r_1"], &[("PINRAIL_URL", "http://127.0.0.1:9")]);
    assert!(
        given.contains("http://127.0.0.1:9 (from --url or PINRAIL_URL)"),
        "{given}"
    );
    // the default address, and a command that would have started it: the
    // same words, whichever path found the server missing
    let default = message(
        &["submit", "list", "--title", "t"],
        &[("PINRAIL_PORT", "9")],
    );
    assert!(
        default.contains("http://127.0.0.1:9 (the default)"),
        "{default}"
    );
    assert!(default.contains("PINRAIL_SERVER_CMD"), "{default}");
    let listed = message(&["show", "r_1"], &[("PINRAIL_PORT", "9")]);
    assert_eq!(
        listed.split(':').next(),
        default.split(':').next(),
        "{listed} / {default}"
    );
}

/// A folder of the test's own, removed when the test ends.
struct TempDir(std::path::PathBuf);

impl std::ops::Deref for TempDir {
    type Target = std::path::PathBuf;
    fn deref(&self) -> &std::path::PathBuf {
        &self.0
    }
}

impl AsRef<std::path::Path> for TempDir {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::ffi::OsStr> for TempDir {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.0.as_os_str()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tempdir() -> TempDir {
    let dir = std::env::temp_dir().join(format!(
        "pinrail-cli-test-{}-{}",
        std::process::id(),
        rand_suffix()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    TempDir(dir)
}

fn rand_suffix() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

#[test]
fn describe_gives_one_plugin_whole_and_says_when_one_is_broken() {
    let server = MockServer::start(Box::new(|method, path, _| {
        assert_eq!(method, "GET");
        let body = r#"{"plugins":[{"name":"list","title":"List","release":"1.2.0","description":"Items to accept or reject.","use_when":"Before posting review comments","payload_schema":{"type":"object"},"decision_schema":{"type":"object"},"example":{"groups":[]},"markdown":true}]}"#;
        match path {
            "/api/v1/plugins/list/describe" => (200, body.into()),
            "/api/v1/plugins" => (200, r#"{"plugins":[{"name":"hello","path":"/src/hello","error":"entry view/index.html not found"}]}"#.into()),
            _ => (
                404,
                r#"{"error":"not_found","message":"plugin nope not found","violations":[]}"#.into(),
            ),
        }
    }));
    // JSON is the app's description, as it gives it
    let (code, stdout, stderr) = run(&server, &["plugins", "describe", "list"]);
    assert_eq!(code, 0, "{stderr}");
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(doc["name"], "list", "the plugin itself, not a list of one");
    assert_eq!(doc["use_when"], "Before posting review comments");
    assert!(doc.get("submit").is_none(), "{doc}");

    // one plugin is the document: its heading first, its command, no general parts
    let (code, stdout, _) = run(&server, &["plugins", "describe", "list", "--markdown"]);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("# List (`list`) · 1.2.0\n"), "{stdout}");
    assert!(stdout.contains("**Use when:** Before posting review comments"));
    assert!(stdout.contains("pinrail submit list --title"));
    assert!(!stdout.contains("| 4 | timed out") && !stdout.contains("# Pinrail plugins"));
    // the decision's schema only when asked for; it reads as markdown otherwise
    assert!(
        stdout.contains("pinrail plugins describe list --decision-schema"),
        "{stdout}"
    );
    assert!(!stdout.contains("is shaped by"));
    // --decision-schema: the schema alone, JSON either way
    let (_, stdout, _) = run(
        &server,
        &[
            "plugins",
            "describe",
            "list",
            "--decision-schema",
            "--markdown",
        ],
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&stdout).unwrap(),
        serde_json::json!({"type":"object"})
    );
    let (_, stdout, _) = run(
        &server,
        &[
            "plugins",
            "describe",
            "list",
            "--payload-schema",
            "--markdown",
        ],
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&stdout).unwrap(),
        serde_json::json!({"type":"object"})
    );
    let (_, stdout, _) = run(&server, &["plugins", "describe", "list", "--example"]);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&stdout).unwrap(),
        serde_json::json!({"groups":[]})
    );
    let (code, _, _) = run(
        &server,
        &[
            "plugins",
            "describe",
            "list",
            "--example",
            "--payload-schema",
        ],
    );
    assert_eq!(code, 1, "one part at a time");

    // a plugin's name
    let (code, _, stderr) = run(&server, &["plugins", "describe"]);
    assert_eq!(code, 1, "bad arguments, not refused");
    assert!(stderr.contains("<NAME>"), "{stderr}");

    let (code, _, stderr) = run(&server, &["plugins", "describe", "nope", "--markdown"]);
    assert_eq!(code, 2);
    assert_eq!(
        stderr,
        "pinrail: refused: plugin nope not found\nInstalled plugins: pinrail plugins\n"
    );

    // installed but broken: said so, with where to look
    let (code, _, stderr) = run(&server, &["plugins", "describe", "hello", "--markdown"]);
    assert_eq!(code, 2);
    assert_eq!(
        stderr,
        "pinrail: refused: plugin hello is installed but can't be used: entry view/index.html not found\n\
         What to fix: pinrail plugins check /src/hello\n"
    );
}

#[test]
fn a_dry_run_checks_the_submission_and_creates_nothing() {
    let server = MockServer::start(Box::new(|method, path, body| {
        assert_eq!((method, path), ("POST", "/api/v1/reviews/validate"));
        let sent: serde_json::Value = serde_json::from_str(body).unwrap();
        if sent["title"] == "" {
            return (422, r#"{"error":"invalid","message":"validation failed","violations":[{"path":"/title","message":"is required"}]}"#.into());
        }
        (
            200,
            r#"{"valid":true,"plugin":"list","plugin_version":1,"plugin_release":"1.2.0"}"#.into(),
        )
    }));
    let (code, stdout, stderr) = run(
        &server,
        &["submit", "list", "--title", "t", "--dry-run", "--no-start"],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains(r#""valid":true"#), "{stdout}");
    assert!(stderr.is_empty(), "{stderr}");

    let (code, _, stderr) = run(
        &server,
        &["submit", "list", "--title", "", "--dry-run", "--no-start"],
    );
    assert_eq!(code, 2);
    assert!(stderr.contains(r#""path":"/title""#), "{stderr}");

    let (code, _, _) = run(
        &server,
        &["submit", "list", "--title", "t", "--dry-run", "--wait"],
    );
    assert_eq!(code, 1, "--dry-run with --wait is bad arguments");
}

#[test]
fn a_request_file_is_the_body_and_flags_override_its_keys() {
    let sent = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let seen = sent.clone();
    let server = MockServer::start(Box::new(move |_, _, body| {
        seen.lock()
            .unwrap()
            .push(serde_json::from_str(body).unwrap());
        (201, review("pending"))
    }));
    let dir = tempdir();
    let request = dir.join("request.json");
    std::fs::write(
        &request,
        r#"{"plugin":"list","title":"Sentry triage","origin":{"repo":"acme"},"requested_by":"agent","payload":{"groups":[]}}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("p.json"),
        r#"{"groups":[{"title":"x","items":[]}]}"#,
    )
    .unwrap();

    let (code, _, stderr) = run(
        &server,
        &[
            "submit",
            "--request",
            request.to_str().unwrap(),
            "--no-start",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    let (code, _, stderr) = run(
        &server,
        &[
            "submit",
            "--request",
            request.to_str().unwrap(),
            "--title",
            "Sentry triage, round 2",
            "--revises",
            "r_1",
            "--data",
            dir.join("p.json").to_str().unwrap(),
            "--no-start",
        ],
    );
    assert_eq!(code, 0, "{stderr}");

    let sent = sent.lock().unwrap();
    assert_eq!(sent[0]["plugin"], "list");
    assert_eq!(sent[0]["title"], "Sentry triage");
    assert_eq!(sent[0]["origin"]["repo"], "acme");
    assert_eq!(
        sent[0]["requested_by"], "agent",
        "the file's, not the default"
    );
    assert_eq!(sent[1]["title"], "Sentry triage, round 2");
    assert_eq!(sent[1]["revises"], "r_1");
    assert_eq!(sent[1]["payload"]["groups"][0]["title"], "x");
    assert_eq!(
        sent[1]["origin"]["repo"], "acme",
        "the rest of the file stays"
    );

    std::fs::write(&request, "[]").unwrap();
    let (code, _, stderr) = run(
        &server,
        &[
            "submit",
            "--request",
            request.to_str().unwrap(),
            "--no-start",
        ],
    );
    assert_eq!(code, 1);
    assert!(stderr.contains("must hold a JSON object"), "{stderr}");
    let (code, _, _) = run(&server, &["submit", "list", "--no-start"]);
    assert_eq!(code, 1, "--title is wanted without --request");
}

#[test]
fn a_refusal_in_plain_text_is_reported_as_a_refusal() {
    // what something in front of the server, or an older server, sends
    let server = MockServer::start(Box::new(|_, _, _| {
        (
            413,
            "Failed to buffer the request body: length limit exceeded".into(),
        )
    }));
    let dir = tempdir();
    std::fs::write(dir.join("p.json"), r#"{"groups":[]}"#).unwrap();
    let (code, _, stderr) = run(
        &server,
        &[
            "submit",
            "list",
            "--title",
            "t",
            "--data",
            dir.join("p.json").to_str().unwrap(),
            "--no-start",
        ],
    );
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("length limit exceeded"), "{stderr}");
    assert!(!stderr.contains("invalid JSON"), "{stderr}");
}

fn sha256(text: &str) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(text.as_bytes()))
}

#[test]
fn submit_checks_then_uploads_only_what_the_app_lacks() {
    let pivot = sha256("pivot glb");
    let column = sha256("column glb");
    let sent = Arc::new(Mutex::new(Vec::<(String, String, String)>::new()));
    let seen = sent.clone();
    let (have, missing) = (column.clone(), pivot.clone());
    let server = MockServer::start(Box::new(move |method, path, body| {
        seen.lock()
            .unwrap()
            .push((method.into(), path.into(), body.into()));
        match (method, path) {
            ("POST", "/api/v1/reviews/validate") => (
                200,
                r#"{"valid":true,"plugin":"model","plugin_release":"2.0.0"}"#.into(),
            ),
            ("HEAD", p) if p.ends_with(&have) => (200, String::new()),
            ("HEAD", p) if p.ends_with(&missing) => (404, String::new()),
            ("PUT", p) if p.ends_with(&missing) => {
                (201, format!(r#"{{"sha256":"{missing}","size":9}}"#))
            }
            ("POST", "/api/v1/reviews") => (201, review("pending")),
            other => panic!("unexpected {other:?}"),
        }
    }));
    let dir = tempdir();
    std::fs::write(
        dir.join("p.json"),
        r#"{"models":[{"id":"L1","name":"Pivot","file":{"$attachment":"pivot.glb"}}]}"#,
    )
    .unwrap();
    std::fs::write(dir.join("pivot.glb"), "pivot glb").unwrap();
    std::fs::write(dir.join("v2.glb"), "column glb").unwrap();
    let (code, _, stderr) = run(
        &server,
        &[
            "submit",
            "model",
            "--title",
            "t",
            "--no-start",
            "--data",
            dir.join("p.json").to_str().unwrap(),
            "--attach",
            dir.join("pivot.glb").to_str().unwrap(),
            "--attach",
            &format!("{}=column.glb", dir.join("v2.glb").display()),
            "--verbose",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains("uploading pivot.glb (9 bytes)"), "{stderr}");
    assert!(!stderr.contains("uploading column.glb"), "{stderr}");

    let sent = sent.lock().unwrap();
    let order: Vec<String> = sent
        .iter()
        .map(|(m, p, _)| {
            format!(
                "{m} {}",
                p.replace(&pivot, "{pivot}").replace(&column, "{column}")
            )
        })
        .collect();
    assert_eq!(
        order,
        [
            "POST /api/v1/reviews/validate",
            "HEAD /api/v1/attachments/{column}",
            "HEAD /api/v1/attachments/{pivot}",
            "PUT /api/v1/attachments/{pivot}",
            "POST /api/v1/reviews",
        ]
    );
    assert_eq!(sent[3].2, "pivot glb", "the file's bytes, as they are");
    let submitted: serde_json::Value = serde_json::from_str(&sent[4].2).unwrap();
    assert_eq!(
        submitted["attachments"],
        serde_json::json!({
            "column.glb": { "sha256": column, "size": 10, "media_type": "model/gltf-binary" },
            "pivot.glb": { "sha256": pivot, "size": 9, "media_type": "model/gltf-binary" },
        })
    );
}

#[test]
fn a_dry_run_or_a_refused_submission_uploads_nothing() {
    let refuse = Arc::new(Mutex::new(false));
    let refusing = refuse.clone();
    let server = MockServer::start(Box::new(move |method, path, _| {
        match (method, path) {
        ("POST", "/api/v1/reviews/validate") if *refusing.lock().unwrap() => (
            422,
            r#"{"error":"invalid","message":"validation failed","violations":[{"path":"/attachments/a.glb","message":"this plugin takes .png, not model/gltf-binary"}]}"#.into(),
        ),
        ("POST", "/api/v1/reviews/validate") => (200, r#"{"valid":true,"plugin":"model","plugin_release":"2.0.0"}"#.into()),
        other => panic!("unexpected {other:?}"),
    }
    }));
    let dir = tempdir();
    std::fs::write(dir.join("a.glb"), "a").unwrap();
    // the request file names its files relative to itself
    std::fs::write(
        dir.join("request.json"),
        r#"{"plugin":"model","title":"t","payload":{},"attachments":{"a.glb":"a.glb"}}"#,
    )
    .unwrap();
    let request = dir.join("request.json");
    let args = [
        "submit",
        "--request",
        request.to_str().unwrap(),
        "--no-start",
    ];

    let (code, _, stderr) = run(&server, &[&args[..], &["--dry-run"]].concat());
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(server.requests(), ["POST /api/v1/reviews/validate"]);

    *refuse.lock().unwrap() = true;
    let (code, _, stderr) = run(&server, &args);
    assert_eq!(code, 2);
    assert!(stderr.contains("this plugin takes .png"), "{stderr}");
    assert_eq!(
        server.requests().len(),
        2,
        "no HEAD, no PUT: {:?}",
        server.requests()
    );
}

#[test]
fn a_missing_file_or_two_flags_with_one_name_stop_before_anything_is_sent() {
    let server = MockServer::start(Box::new(|method, path, _| {
        panic!("unexpected {method} {path}")
    }));
    let dir = tempdir();
    std::fs::write(dir.join("a.glb"), "a").unwrap();
    let a = dir.join("a.glb");
    let (code, _, stderr) = run(
        &server,
        &[
            "submit",
            "model",
            "--title",
            "t",
            "--no-start",
            "--attach",
            "nowhere/x.glb",
        ],
    );
    assert_eq!(code, 1);
    assert!(stderr.contains("reading nowhere/x.glb"), "{stderr}");
    let (code, _, stderr) = run(
        &server,
        &[
            "submit",
            "model",
            "--title",
            "t",
            "--no-start",
            "--attach",
            a.to_str().unwrap(),
            "--attach",
            a.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 1);
    assert!(stderr.contains("two --attach flags name a.glb"), "{stderr}");
    assert!(server.requests().is_empty());
}

#[test]
fn attachments_lists_a_review_s_files_and_saves_one() {
    let server = MockServer::start(Box::new(|method, path, _| {
        match (method, path) {
        ("GET", "/api/v1/reviews/r_1") => (200, r#"{"id":"r_1","attachments":[{"name":"Pivot lamp.glb","size":9,"media_type":"model/gltf-binary","sha256":"ab"}]}"#.into()),
        ("GET", "/api/v1/reviews/r_1/attachments/Pivot%20lamp.glb") => (200, "pivot glb".into()),
        ("GET", "/api/v1/reviews/r_1/attachments/nope.glb") => (404, r#"{"error":"not_found","message":"review r_1 carries no attachment \"nope.glb\"","violations":[]}"#.into()),
        other => panic!("unexpected {other:?}"),
    }
    }));
    let (code, stdout, _) = run(&server, &["attachments", "list", "r_1"]);
    assert_eq!(code, 0);
    assert!(stdout.contains(r#""name":"Pivot lamp.glb""#), "{stdout}");

    let dir = tempdir();
    let to = dir.join("lamp.glb");
    let (code, _, stderr) = run(
        &server,
        &[
            "attachments",
            "get",
            "r_1",
            "Pivot lamp.glb",
            "-o",
            to.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "pivot glb");
    // into a folder, under the file's own name
    let folder = dir.join("downloads");
    std::fs::create_dir(&folder).unwrap();
    let (code, _, stderr) = run(
        &server,
        &[
            "attachments",
            "get",
            "r_1",
            "Pivot lamp.glb",
            "-o",
            folder.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        std::fs::read_to_string(folder.join("Pivot lamp.glb")).unwrap(),
        "pivot glb"
    );
    // not over a file that is there, unless asked
    let (code, _, stderr) = run(
        &server,
        &[
            "attachments",
            "get",
            "r_1",
            "Pivot lamp.glb",
            "-o",
            to.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 1);
    assert!(stderr.contains("--force"), "{stderr}");
    let (code, _, _) = run(
        &server,
        &[
            "attachments",
            "get",
            "r_1",
            "Pivot lamp.glb",
            "-o",
            to.to_str().unwrap(),
            "--force",
        ],
    );
    assert_eq!(code, 0);
    // a name the review does not carry leaves nothing behind
    let missing = dir.join("nope.glb");
    let (code, _, stderr) = run(
        &server,
        &[
            "attachments",
            "get",
            "r_1",
            "nope.glb",
            "-o",
            missing.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 2);
    assert!(stderr.contains("carries no attachment"), "{stderr}");
    assert!(!missing.exists());
    // lamp.glb and the downloads folder, and no .part left
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2, "no .part left");
}

#[test]
fn plugins_and_describe_say_what_files_a_plugin_takes_and_how_to_send_them() {
    let server = MockServer::start(Box::new(|_, path, _| {
        let body = r#"{"plugins":[
            {"name":"model","title":"3D model review","release":"2.0.0","payload_schema":{},"decision_schema":{},"example":null,"markdown":true,
             "install":null,"attachments":{"accept":[".glb","model/gltf-binary"],"max_size":52428800,"max_count":12}},
            {"name":"list","title":"List","release":"1.0.0","payload_schema":{},"decision_schema":{},"example":null,"markdown":true,"install":null,"attachments":null}]}"#;
        match path {
            "/api/v1/plugins" => (200, body.into()),
            // describe answers with the one plugin asked for
            "/api/v1/plugins/model/describe" => {
                let all: serde_json::Value = serde_json::from_str(body).unwrap();
                (
                    200,
                    serde_json::json!({ "plugins": [all["plugins"][0]] }).to_string(),
                )
            }
            other => panic!("unexpected {other}"),
        }
    }));
    // choosing: the listing says which kinds, extensions first
    let (code, stdout, stderr) = run(&server, &["plugins", "--markdown"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("- model · 2.0.0 · built in · ready\n  Takes files: .glb.\n"),
        "{stdout}"
    );
    assert_eq!(stdout.matches("Takes files").count(), 1);

    // in full: the kinds, the limits and how to send them
    let (code, stdout, _) = run(&server, &["plugins", "describe", "model", "--markdown"]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("## Files\n\nTakes files beside the payload: .glb, model/gltf-binary (up to 50 MB each, 12 at most)."),
        "{stdout}"
    );
    assert!(stdout.contains("pinrail submit model --title \"<what it is about>\" --data payload.json --attach <file> --wait"));
    assert_eq!(stdout.matches("## Files").count(), 1);
}

#[test]
fn describe_gives_a_file_limit_under_a_megabyte_as_it_is() {
    // a plugin that takes small files: 500 KB must not read as "up to 0 MB"
    let server = MockServer::start(Box::new(|_, path, _| {
        assert_eq!(path, "/api/v1/plugins/notes/describe");
        (
            200,
            r#"{"plugins":[{"name":"notes","title":"Notes","release":"1.0.0","payload_schema":{},"decision_schema":{},"example":null,"markdown":true,
                "install":null,"attachments":{"accept":[".txt"],"max_size":512000}}]}"#
                .into(),
        )
    }));
    let (code, stdout, stderr) = run(&server, &["plugins", "describe", "notes", "--markdown"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("(up to 500 KB each)"), "{stdout}");
}

#[test]
fn submit_sample_asks_for_the_plugins_sample_and_nothing_else() {
    let server = MockServer::start(Box::new(|method, path, body| {
        assert_eq!((method, path), ("POST", "/api/v1/plugins/list/sample"));
        let sent: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(
            sent,
            serde_json::json!({ "title": "Try Pinrail", "requested_by": "pinrail-cli" })
        );
        (201, review("pending"))
    }));
    let (code, stdout, stderr) = run(
        &server,
        &["submit", "list", "--sample", "--title", "Try Pinrail"],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("\"status\":\"pending\""));
    assert!(stderr.contains("review r_1 submitted"), "{stderr}");

    // a payload of one's own is not a sample
    let (code, _, stderr) = run(&server, &["submit", "list", "--sample", "--data", "p.json"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("cannot be used with"), "{stderr}");
}

/// Each line of a listing sums the review up as its plugin declares: what
/// a pending review asks, what an ended one decided.
#[test]
fn list_lines_carry_each_reviews_summary() {
    let server = MockServer::start(Box::new(|_, _, _| {
        (
            200,
            r#"{"reviews":[
                {"id":"r_1","plugin":"list","title":"Tickets","status":"pending","created_at":"2026-09-11T10:00:00Z",
                 "summary":{"counts":[{"label":"major","count":2,"tone":"warning"},{"label":"nit","count":1,"tone":"neutral"}]}},
                {"id":"r_2","plugin":"calendar","title":"Week","status":"decided","created_at":"2026-09-11T10:00:00Z",
                 "summary":{"counts":[{"label":"events","count":3,"tone":"neutral"}]},
                 "decision":{"decided_by":"tester","decided_at":"2026-09-11T11:00:00Z","data":{},
                   "summary":{"verdict":{"label":"approved","tone":"success"},"counts":[{"label":"scheduled","count":3,"tone":"success"}]}}}
            ],"total":2,"has_more":false}"#
                .into(),
        )
    }));
    let (code, stdout, stderr) = run(&server, &["list", "--all", "--markdown"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("- r_1 · pending · list · Tickets · 2 major, 1 nit · "),
        "{stdout}"
    );
    assert!(
        stdout.contains("- r_2 · decided · calendar · Week · approved, 3 scheduled · "),
        "{stdout}"
    );
}

#[test]
fn list_shows_the_pending_reviews_of_the_checkout_and_all_with_all() {
    let dir = tempdir();
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["remote", "add", "origin", "git@github.com:acme/api.git"]);
    let asked = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = asked.clone();
    let server = MockServer::start(Box::new(move |_, path, _| {
        seen.lock().unwrap().push(path.to_string());
        (
            200,
            format!(
                r#"{{"reviews":[{}],"total":3,"has_more":true,"next_cursor":"r_1"}}"#,
                review("pending")
            ),
        )
    }));

    let (code, stdout, stderr) = run_in(&server, &dir, &["list", "--markdown"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.starts_with("1 of 3 pending reviews in acme/api, newest first; --all for every review.\n\n- r_1 · pending · "),
        "{stdout}"
    );
    assert!(stdout.ends_with("\nMore: --cursor r_1\n"), "{stdout}");

    run_in(&server, &dir, &["list", "--status", "decided"]);
    let asked = asked.lock().unwrap();
    assert_eq!(asked[0], "/api/v1/reviews?status=pending&repo=acme%2Fapi");
    assert_eq!(asked[1], "/api/v1/reviews?status=decided&repo=acme%2Fapi");
}

#[test]
fn list_all_asks_for_every_review_and_every_page() {
    let server = MockServer::start(Box::new(|_, path, _| {
        assert_eq!(path, "/api/v1/reviews");
        (200, r#"{"reviews":[],"total":0,"has_more":false}"#.into())
    }));
    let (code, stdout, stderr) = run(&server, &["list", "--all", "--markdown"]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "0 reviews.\n");
}

#[test]
fn list_all_with_a_limit_prints_that_many_and_where_the_rest_start() {
    let server = MockServer::start(Box::new(|_, path, _| {
        assert_eq!(path, "/api/v1/reviews?limit=1");
        (
            200,
            format!(
                r#"{{"reviews":[{}],"total":3,"has_more":true,"next_cursor":"r_1"}}"#,
                review("decided")
            ),
        )
    }));
    let (code, stdout, stderr) = run(&server, &["list", "--all", "--limit", "1", "--markdown"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.starts_with("1 of 3 reviews, newest first.\n\n- r_1 · decided · "),
        "{stdout}"
    );
    assert!(stdout.ends_with("\nMore: --cursor r_1\n"), "{stdout}");
}

#[test]
fn submit_fills_the_origin_from_the_git_checkout_it_runs_in() {
    let dir = tempdir();
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q", "-b", "fix/tickets"]);
    git(&["remote", "add", "origin", "git@github.com:acme/api.git"]);
    std::fs::write(dir.join("p.json"), r#"{"groups":[]}"#).unwrap();
    let sent = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let seen = sent.clone();
    let server = MockServer::start(Box::new(move |_, _, body| {
        seen.lock()
            .unwrap()
            .push(serde_json::from_str(body).unwrap());
        (201, review("pending"))
    }));

    let (code, _, stderr) = run_in(
        &server,
        &dir,
        &["submit", "list", "--title", "t", "--data", "p.json", "-v"],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stderr.contains("pinrail: origin from git: repo=acme/api, ref=fix/tickets"),
        "{stderr}"
    );

    // what the agent says wins; only what it leaves out is filled
    let (code, _, stderr) = run_in(
        &server,
        &dir,
        &[
            "submit",
            "list",
            "--title",
            "t",
            "--data",
            "p.json",
            "--origin",
            "repo=acme/web,url=https://x",
        ],
    );
    assert_eq!(code, 0, "{stderr}");

    let sent = sent.lock().unwrap();
    assert_eq!(
        sent[0]["origin"],
        serde_json::json!({ "repo": "acme/api", "ref": "fix/tickets" })
    );
    // another project than the checkout's: its branch is not this one's
    assert_eq!(
        sent[1]["origin"],
        serde_json::json!({ "repo": "acme/web", "url": "https://x" })
    );
}

#[test]
fn plugins_as_markdown_is_a_line_a_plugin() {
    let server = MockServer::start(Box::new(|_, path, _| {
        assert_eq!(path, "/api/v1/plugins");
        (200, r#"{"plugins":[
            {"name":"list","release":"1.0.0","install":null,"error":null,"description":"Proposed actions to accept or reject.","use_when":"You have changes to propose."},
            {"name":"review","release":"2.1.0","install":{"linked":true,"source":"/src/review"},"error":null},
            {"name":"odd","release":"0.1.0","install":{"linked":false,"source":"github.com/acme/odd"},"error":"entry index.html not found"}]}"#.into())
    }));
    let (code, stdout, stderr) = run(&server, &["plugins", "--markdown"]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        stdout,
        "3 plugins installed. pinrail plugins describe <name> shows a plugin's payload schema and an example, and --decision-schema shows what it returns.\n\n\
         - list · 1.0.0 · built in · ready\n\
         \x20 Proposed actions to accept or reject.\n\
         \x20 Use when: You have changes to propose.\n\
         - review · 2.1.0 · linked, /src/review · ready\n\
         - odd · 0.1.0 · github.com/acme/odd · broken: entry index.html not found\n"
    );
}

#[test]
fn plugins_install_sends_a_folder_as_its_full_path_with_dotdot_resolved() {
    let dir = tempdir();
    std::fs::create_dir_all(dir.join("work")).unwrap();
    std::fs::create_dir_all(dir.join("plugins/hello")).unwrap();
    let expected = dir
        .join("plugins/hello")
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let sent = Arc::new(Mutex::new(String::new()));
    let seen = sent.clone();
    let server = MockServer::start(Box::new(move |method, path, body| {
        match (method, path) {
        ("POST", "/api/v1/plugins/install") => {
            let body: serde_json::Value = serde_json::from_str(body).unwrap();
            *seen.lock().unwrap() = body["source"].as_str().unwrap().to_string();
            (202, r#"{"job":"j1"}"#.into())
        }
        ("GET", "/api/v1/plugins/jobs/j1") => (
            200,
            r#"{"status":"done","log":"","plugin":{"name":"hello","release":"1.0.0","entry":"view/index.html"}}"#.into(),
        ),
        other => panic!("unexpected {other:?}"),
    }
    }));
    let (code, _, stderr) = run_in(
        &server,
        &dir.join("work"),
        &["plugins", "install", "../plugins/hello", "--link"],
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(*sent.lock().unwrap(), expected);
}

#[test]
fn an_install_log_the_app_trims_is_followed_line_by_line() {
    // three polls of a build: the app keeps only the end of the log and
    // says how much it dropped from the start; the output is not ASCII
    let lines = ["▶ one é", "▶ two ─", "▶ three →", "▶ four ✓"];
    let text = |from: usize, to: usize| {
        lines[from..to]
            .iter()
            .map(|l| format!("{l}\n"))
            .collect::<String>()
    };
    let offset = |n: usize| text(0, n).len();
    let polls = Arc::new(Mutex::new(vec![
        serde_json::json!({ "status": "building", "log": text(0, 2), "log_offset": 0 }),
        serde_json::json!({ "status": "building", "log": text(1, 3), "log_offset": offset(1) }),
        serde_json::json!({ "status": "done", "log": text(2, 4), "log_offset": offset(2),
            "plugin": { "name": "hello", "release": "1.0.0", "entry": "view/index.html" } }),
    ]));
    let server = MockServer::start(Box::new(move |method, path, _| match (method, path) {
        ("POST", "/api/v1/plugins/inspect") => (200, INSPECTED_WITHOUT_BUILD.into()),
        ("POST", "/api/v1/plugins/install") => (202, r#"{"job":"j1"}"#.into()),
        ("GET", "/api/v1/plugins/jobs/j1") => {
            let mut polls = polls.lock().unwrap();
            let next = if polls.len() > 1 {
                polls.remove(0)
            } else {
                polls[0].clone()
            };
            (200, next.to_string())
        }
        other => panic!("unexpected {other:?}"),
    }));
    let dir = tempdir();
    std::fs::create_dir_all(dir.join("hello")).unwrap();
    let (code, _, stderr) = run_in(&server, &dir, &["plugins", "install", "hello"]);
    assert_eq!(code, 0, "{stderr}");
    let printed: Vec<&str> = stderr.lines().filter(|l| l.starts_with('▶')).collect();
    assert_eq!(printed, lines, "{stderr}");
}

#[test]
fn plugins_new_prints_what_it_wrote_and_the_next_steps_on_stdout() {
    let dir = tempdir();
    let server = MockServer::start(Box::new(|_, path, _| {
        panic!("no app needed, asked for {path}")
    }));
    let (code, stdout, stderr) = run_in(&server, &dir, &["plugins", "new", "triage", "--markdown"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    let written = dir.join("triage").canonicalize().unwrap();
    assert!(
        stdout.starts_with(&format!(
            "triage written to {}.\n\nNext:\n  1. Read AGENTS.md in triage",
            written.display()
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "  2. pinrail plugins check triage\n  3. pinrail plugins install triage --link"
        ),
        "{stdout}"
    );
    assert!(stdout.contains("How a plugin works: pinrail docs plugins/building\n"));

    let (code, stdout, _) = run_in(&server, &dir, &["plugins", "new", "other"]);
    assert_eq!(code, 0);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["linked"], false);
    assert_eq!(json["next"][1], "pinrail plugins check other");
}

#[test]
fn plugins_update_without_a_name_says_what_became_of_each_plugin() {
    let server = MockServer::start(Box::new(|method, path, _| {
        match (method, path) {
        ("GET", "/api/v1/plugins") => (200, r#"{"plugins":[
            {"name":"list","release":"1.0.0","install":null},
            {"name":"review","release":"2.1.0","install":{"linked":true,"source":"/src/review"}},
            {"name":"odd","release":"0.1.0","install":{"linked":false,"source":"github.com/acme/odd"}}]}"#.into()),
        ("POST", "/api/v1/plugins/odd/update/inspect") => (200, r#"{"state":"up_to_date","version":"0.1.0"}"#.into()),
        other => panic!("unexpected {other:?}"),
    }
    }));
    let (code, stdout, stderr) = run(&server, &["plugins", "update", "--markdown"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(
        stdout,
        "list: built in, updated with the app\n\
         review: linked, served live from /src/review\n\
         odd: up to date, 0.1.0\n"
    );
}

#[test]
fn plugins_update_without_a_name_reports_every_plugin_when_one_fails() {
    // one failure in the middle: what was updated before it, and what comes
    // after it, still has to be said
    let server = MockServer::start(Box::new(|method, path, _| {
        match (method, path) {
        ("GET", "/api/v1/plugins") => (200, r#"{"plugins":[
            {"name":"first","release":"1.0.0","install":{"linked":false,"source":"github.com/acme/first"}},
            {"name":"broken","release":"1.0.0","install":{"linked":false,"source":"github.com/acme/broken"}},
            {"name":"last","release":"1.0.0","install":{"linked":false,"source":"github.com/acme/last"}}]}"#.into()),
        ("POST", "/api/v1/plugins/first/update/inspect") => (200, r#"{"state":"up_to_date","version":"1.0.0"}"#.into()),
        ("POST", "/api/v1/plugins/broken/update/inspect") => (422, r#"{"error":"invalid","message":"github.com/acme/broken could not be fetched","violations":[]}"#.into()),
        ("POST", "/api/v1/plugins/last/update/inspect") => (200, r#"{"state":"up_to_date","version":"1.0.0"}"#.into()),
        other => panic!("unexpected {other:?}"),
    }
    }));
    let (code, stdout, stderr) = run(&server, &["plugins", "update", "--markdown"]);
    assert_eq!(code, 2, "a failure is still a failure: {stderr}");
    assert_eq!(
        stdout,
        "first: up to date, 1.0.0\n\
         broken: failed: github.com/acme/broken could not be fetched\n\
         last: up to date, 1.0.0\n"
    );
}

/// The command with no app to talk to: the address it would use answers
/// nothing.
fn run_offline(args: &[&str]) -> (i32, String, String) {
    let markdown = args.contains(&"--markdown");
    let args: Vec<&str> = args
        .iter()
        .copied()
        .filter(|a| *a != "--markdown")
        .collect();
    let mut cmd = pinrail();
    if markdown {
        cmd.env_remove("PINRAIL_JSON");
    }
    let out = cmd
        .args(&args)
        .env("PINRAIL_URL", "http://127.0.0.1:9")
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn repo_plugin(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../plugins")
        .join(name)
        .canonicalize()
        .unwrap()
}

/// The check is the format's own, run by the command: no app is needed,
/// and the exit code is the verdict.
#[test]
fn plugins_check_needs_no_app_and_exits_by_its_verdict() {
    let list = repo_plugin("list");
    let (code, stdout, stderr) = run_offline(&["plugins", "check", list.to_str().unwrap()]);
    assert_eq!(code, 0, "{stderr}");
    let verdict: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(verdict["usable"], true, "{verdict}");
    assert_eq!(verdict["name"], "list");
    assert_eq!(verdict["problems"], serde_json::json!([]));
    assert_eq!(verdict["warnings"], serde_json::json!([]));

    // a missing entry costs the plugin; a broken sample, the sample
    let dir = tempdir();
    for f in [
        "manifest.json",
        "schemas/payload.schema.json",
        "schemas/decision.schema.json",
        "templates/decision.md.j2",
        "example.json",
        "icon.svg",
    ] {
        let to = dir.join(f);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(list.join(f), to).unwrap();
    }
    std::fs::write(dir.join("sample.json"), r#"{"payload": {}}"#).unwrap();
    let (code, stdout, _) = run_offline(&["plugins", "check", dir.to_str().unwrap(), "--markdown"]);
    assert_eq!(code, 2);
    assert!(
        stdout.contains("the app would refuse it.\n\n- refused: entry view/index.html not found"),
        "{stdout}"
    );
    std::fs::create_dir_all(dir.join("view")).unwrap();
    std::fs::write(dir.join("view/index.html"), "").unwrap();
    let (code, _, stderr) = run_offline(&["plugins", "check", dir.to_str().unwrap()]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stderr.contains("pinrail: sample dropped: sample.json: needs a title"),
        "{stderr}"
    );
}

/// A plugin as it is scaffolded with a framework, sources and a build but
/// no view yet, is checked as an install would take it: before its build.
#[test]
fn plugins_check_takes_a_plugin_that_builds_its_view_before_its_build() {
    let dir = tempdir();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        r#"{"name": "fresh", "version": "1.0.0", "payload_schema": {}, "decision_schema": {}, "build": {"command": "npm run build"}}"#,
    )
    .unwrap();
    let (code, stdout, stderr) = run_offline(&["plugins", "check", dir.to_str().unwrap()]);
    assert_eq!(code, 0, "{stderr}");
    let verdict: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        verdict["warnings"],
        serde_json::json!([{ "key": "entry", "message": "entry index.html not found yet: the build (npm run build) has to write it" }])
    );
}
