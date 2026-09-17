//! End-to-end tests of the binary against a small scripted HTTP server, so
//! the exit codes and the wait loop are exercised without a real wicket.

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
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        if header.trim().is_empty() {
            break;
        }
        if let Some(v) = header.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).unwrap();
    }
    let body = String::from_utf8_lossy(&body).to_string();
    seen.lock().unwrap().push(format!("{method} {path}"));
    let (status, response) = handler.lock().unwrap()(&method, &path, &body);
    let reason = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        404 => "Not Found",
        409 => "Conflict",
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

fn wicket() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wicket"));
    cmd.env_remove("WICKET_URL")
        .env_remove("WICKET_SERVER_CMD")
        .env_remove("WICKET_SERVER_DIR");
    cmd.stdin(Stdio::null());
    cmd
}

fn run(server: &MockServer, args: &[&str]) -> (i32, String, String) {
    let out = wicket()
        .args(args)
        .env("WICKET_URL", &server.url)
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

    let out = wicket()
        .args(["wait", "r_1", "--timeout", "20"])
        .env("WICKET_URL", &url)
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
        ("GET", "/api/v1/plugins") => (200, r#"{"dirs":[],"plugins":[]}"#.into()),
        ("GET", "/api/v1/plugins/list/versions") => {
            (200, r#"{"name":"list","versions":[1]}"#.into())
        }
        ("POST", "/api/v1/plugins/reload") => (200, r#"{"ok":true,"count":1}"#.into()),
        ("POST", "/api/v1/plugins/dirs") => {
            let sent: serde_json::Value = serde_json::from_str(body).unwrap();
            assert!(sent["dir"].as_str().unwrap().starts_with('/'), "{body}");
            (200, r#"{"ok":true,"count":2,"dirs":[]}"#.into())
        }
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
    assert_eq!(run(&server, &["plugins", "add", "."]).0, 0);
}

#[test]
fn unreachable_server_exits_1_without_auto_start_config() {
    let out = wicket()
        .args(["show", "r_1"])
        .env("WICKET_URL", "http://127.0.0.1:9")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("connecting to the server"));

    let dir = tempdir();
    let out = wicket()
        .args(["submit", "list", "--title", "t"])
        .env("WICKET_URL", "http://127.0.0.1:9")
        .env("WICKET_DATA_DIR", &dir)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    // an explicit --url/WICKET_URL is never auto-started; discovery would be
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
    let out = wicket()
        .args(["show", "r_1"])
        .env("WICKET_DATA_DIR", &dir)
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
        "sleep 0.3; printf '{{\"url\":\"{}\"}}' > \"$WICKET_DATA_DIR/server.json\"; sleep 5",
        server.url
    );
    let out = wicket()
        .args(["submit", "list", "--title", "t"])
        .env("WICKET_DATA_DIR", &dir)
        .env("WICKET_SERVER_CMD", &cmd)
        .env("WICKET_PORT", "9")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(stderr.contains("starting it"), "{stderr}");
    assert!(dir.join("server.log").exists());
}

fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "wicket-cli-test-{}-{}",
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
