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
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
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

fn pinrail() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pinrail"));
    cmd.env_remove("PINRAIL_URL")
        .env_remove("PINRAIL_SERVER_CMD");
    cmd.stdin(Stdio::null());
    cmd
}

fn run(server: &MockServer, args: &[&str]) -> (i32, String, String) {
    let out = pinrail()
        .args(args)
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
fn submit_prints_the_review_and_the_url_on_stderr() {
    let server = MockServer::start(Box::new(|method, path, body| {
        assert_eq!((method, path), ("POST", "/api/v1/reviews"));
        let sent: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(sent["plugin"], "list");
        assert_eq!(sent["title"], "MR !42");
        assert_eq!(sent["origin"]["repo"], "acme");
        assert_eq!(sent["origin"]["ref"], "42");
        assert_eq!(sent["payload"]["groups"], serde_json::json!([]));
        assert_eq!(sent["requested_by"], "agent");
        assert_eq!(sent["summary"]["subtitle"], "3 new");
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
            "--summary",
            r#"{"subtitle":"3 new"}"#,
            "--no-start",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.starts_with(r#"{"id":"r_1""#), "{stdout}");
    assert!(
        stderr.contains(&format!("review r_1: {}/reviews/r_1", server.url)),
        "{stderr}"
    );
}

#[test]
fn create_alias_wait_loops_through_204_then_prints_the_decision_and_writes_the_file() {
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
            "create",
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
    // first server dies after answering 204 once; the CLI must retry and
    // finish against the second server on the same URL
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let handler: Arc<Mutex<Handler>> =
            Arc::new(Mutex::new(Box::new(|_, _, _| (204, String::new()))));
        serve_one(stream, handler, Arc::new(Mutex::new(Vec::new())));
        drop(listener);
        thread::sleep(std::time::Duration::from_millis(2500));
        let listener = TcpListener::bind(addr).unwrap();
        let handler: Arc<Mutex<Handler>> =
            Arc::new(Mutex::new(Box::new(|_, _, _| (200, review("decided")))));
        let (stream, _) = listener.accept().unwrap();
        serve_one(stream, handler, Arc::new(Mutex::new(Vec::new())));
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
    assert!(stderr.contains(r#""path": "/title""#), "{stderr}");

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
fn list_all_follows_the_cursor_to_the_last_page() {
    let server = MockServer::start(Box::new(|method, path, _| {
        match (method, path) {
        ("GET", "/api/v1/reviews?status=decided&limit=2") => (
            200,
            r#"{"reviews":[{"id":"r_4"},{"id":"r_3"}],"total":5,"has_more":true,"next_cursor":"r_3"}"#.into(),
        ),
        ("GET", "/api/v1/reviews?status=decided&limit=2&cursor=r_3") => (
            200,
            r#"{"reviews":[{"id":"r_2"},{"id":"r_1"}],"total":5,"has_more":true,"next_cursor":"r_1"}"#.into(),
        ),
        ("GET", "/api/v1/reviews?status=decided&limit=2&cursor=r_1") => (
            200,
            r#"{"reviews":[{"id":"r_0"}],"total":5,"has_more":false,"next_cursor":null}"#.into(),
        ),
        other => panic!("unexpected {other:?}"),
    }
    }));
    let (code, stdout, stderr) = run(
        &server,
        &["list", "--status", "decided", "--limit", "2", "--all"],
    );
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
    let (code, stdout, _) = run(&server, &["list", "--status", "decided", "--limit", "2"]);
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
        ("GET", "/api/v1/reviews?include_revised=true") => (
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
    assert_eq!(run(&server, &["types"]).0, 0);
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
    assert!(String::from_utf8_lossy(&out.stderr).contains("connecting to the server"));

    let dir = tempdir();
    let out = pinrail()
        .args(["submit", "list", "--title", "t"])
        .env("PINRAIL_URL", "http://127.0.0.1:9")
        .env("PINRAIL_DATA_DIR", &dir)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    // an explicit --url/PINRAIL_URL is never auto-started; discovery would be
    assert!(String::from_utf8_lossy(&out.stderr).contains("connecting to the server"));
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
        .args(["submit", "list", "--title", "t"])
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

fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pinrail-cli-test-{}-{}",
        std::process::id(),
        rand_suffix()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rand_suffix() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

#[test]
fn describe_adds_how_to_submit_and_the_exit_codes_to_the_plugins() {
    let server = MockServer::start(Box::new(|method, path, _| {
        assert_eq!(method, "GET");
        let body = r#"{"plugins":[{"name":"list","title":"List","release":"1.2.0","description":"Items to accept or reject.","use_when":"Before posting review comments","payload_schema":{"type":"object"},"decision_schema":{"type":"object"},"example":{"groups":[]},"markdown":true}]}"#;
        match path {
            "/api/v1/plugins/describe" | "/api/v1/plugins/list/describe" => (200, body.into()),
            _ => (
                404,
                r#"{"error":"not_found","message":"nope","violations":[]}"#.into(),
            ),
        }
    }));
    let (code, stdout, stderr) = run(&server, &["plugins", "describe"]);
    assert_eq!(code, 0, "{stderr}");
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        doc["plugins"][0]["use_when"],
        "Before posting review comments"
    );
    assert!(
        doc["submit"]["check"]
            .as_str()
            .unwrap()
            .contains("--dry-run")
    );
    assert!(
        doc["submit"]["exit_codes"]["5"]
            .as_str()
            .unwrap()
            .contains("stop")
    );

    let (code, stdout, _) = run(
        &server,
        &["plugins", "describe", "list", "--format", "markdown"],
    );
    assert_eq!(code, 0);
    assert!(stdout.contains("## List (`list`) · 1.2.0"), "{stdout}");
    assert!(stdout.contains("**Use when:** Before posting review comments"));
    assert!(stdout.contains("| 4 | timed out"));

    let (code, _, _) = run(&server, &["plugins", "describe", "nope"]);
    assert_eq!(code, 2);
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|r| r.contains("/describe"))
            .count(),
        3
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
    assert!(stderr.contains("valid; list 1.2.0"), "{stderr}");

    let (code, _, stderr) = run(
        &server,
        &["submit", "list", "--title", "", "--dry-run", "--no-start"],
    );
    assert_eq!(code, 2);
    assert!(stderr.contains(r#""path": "/title""#), "{stderr}");

    let (code, _, _) = run(
        &server,
        &["submit", "list", "--title", "t", "--dry-run", "--wait"],
    );
    assert_eq!(code, 2, "clap refuses --dry-run with --wait");
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
    assert_eq!(code, 2, "clap still wants --title without --request");
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
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no .part left");
}

#[test]
fn describe_says_what_files_a_plugin_takes_and_how_to_send_them() {
    let server = MockServer::start(Box::new(|_, path, _| {
        let body = r#"{"plugins":[
            {"name":"model","title":"3D model review","release":"2.0.0","payload_schema":{},"decision_schema":{},"example":null,"markdown":true,
             "attachments":{"accept":[".glb","model/gltf-binary"],"max_size":52428800,"max_count":12}},
            {"name":"list","title":"List","release":"1.0.0","payload_schema":{},"decision_schema":{},"example":null,"markdown":true,"attachments":null}]}"#;
        match path {
            "/api/v1/plugins/describe" => (200, body.into()),
            other => panic!("unexpected {other}"),
        }
    }));
    let (code, stdout, stderr) = run(&server, &["plugins", "describe"]);
    assert_eq!(code, 0, "{stderr}");
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(doc["plugins"][0]["attachments"]["accept"][0], ".glb");
    assert!(
        doc["submit"]["attachments"]
            .as_str()
            .unwrap()
            .contains("--attach PATH[=NAME]")
    );

    let (code, stdout, _) = run(&server, &["plugins", "describe", "--format", "markdown"]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("### Files\n\nTakes files beside the payload: .glb, model/gltf-binary (up to 50 MB each, 12 at most)."),
        "{stdout}"
    );
    assert!(stdout.contains("pinrail submit model --title \"<what it is about>\" --data payload.json --attach <file> --wait"));
    // a plugin that takes none says nothing about files
    assert_eq!(stdout.matches("### Files").count(), 1);
    assert!(stdout.contains("Files go beside the payload for a plugin that takes them"));
}
