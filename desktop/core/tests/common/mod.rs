//! What the tests that drive the API share: an app on a fresh data
//! directory, and a request through its router.

// each test file uses its own share of these
#![allow(dead_code)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use pinrail_core::api::router;
use pinrail_core::{Config, Pinrail};
use serde_json::Value;
use tower::ServiceExt;

/// A core on a data directory of its own, and its router.
pub struct App {
    /// the data directory, removed when the test ends
    pub dir: tempfile::TempDir,
    pub state: Arc<Pinrail>,
    pub router: Router,
}

/// An app with `list` and `feedback` installed from its catalog, as the
/// person would choose them.
pub fn app() -> App {
    app_with(|_| {})
}

/// An app with nothing installed, as it first opens.
pub fn app_without_plugins() -> App {
    open(|_| {})
}

/// An app whose configuration the test adjusts first, with `list` and
/// `feedback` installed.
pub fn app_with(adjust: impl FnOnce(&mut Config)) -> App {
    let app = open(adjust);
    install(&app, &["forgeplane/list", "forgeplane/feedback"]);
    app
}

/// Installs official plugins from the app's catalog. The install is
/// awaited on a runtime of its own, so a test's runtime can call this.
pub fn install(app: &App, ids: &[&str]) {
    let state = app.state.clone();
    let ids: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for id in ids {
                state.plugins().install_offered(&id).await.unwrap();
            }
        })
    })
    .join()
    .unwrap();
}

/// An app as a person first starts it, with no settings written.
pub fn app_as_new() -> App {
    let app = open(|_| {});
    install(&app, &["forgeplane/list", "forgeplane/feedback"]);
    app
}

fn open(adjust: impl FnOnce(&mut Config)) -> App {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::new(dir.path(), 0);
    config.user = "tester".into();
    config.telemetry_env = false;
    adjust(&mut config);
    let state = Arc::new(Pinrail::open(config).unwrap());
    App {
        router: router(state.clone()),
        state,
        dir,
    }
}

/// Another app opened on a copy of this one's database, as when the person
/// starts the app on data an earlier version wrote.
pub fn open_copy(app: &App) -> App {
    let source = app.state.config().db_path();
    rusqlite::Connection::open(&source)
        .unwrap()
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
    open(|config| {
        let target = config.db_path();
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(&source, &target).unwrap();
    })
}

/// Inspects or seeds the database directly; the app does not expose its
/// connection to callers.
pub fn db(app: &App) -> pinrail_core::db::Db {
    pinrail_core::db::Db::open(&app.state.config().db_path()).unwrap()
}

/// Pinrail's id for the session the agent names by its own id, or that
/// id itself when Pinrail knows no such session.
pub fn sid(app: &App, agent: &str, agent_session: &str) -> String {
    db(app)
        .find_session(agent, agent_session)
        .unwrap()
        .unwrap_or_else(|| agent_session.to_string())
}

/// A request as the app's window sends it: from the app's origin, so a
/// decision or a discard made through it is recorded as the person's, and
/// with the person token. Answered as JSON, or the text when it is not JSON.
pub async fn call(app: &App, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:4747")
        .header("origin", "tauri://localhost")
        .header(
            "authorization",
            format!("Bearer {}", app.state.config().person_token),
        )
        .header("content-type", "application/json")
        .body(
            body.map(|b| Body::from(b.to_string()))
                .unwrap_or_else(Body::empty),
        )
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    (status, value)
}

/// A request from a program on this machine, such as the CLI: no origin
/// and no person token, with these extra headers.
pub async fn call_as(
    app: &App,
    method: &str,
    path: &str,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:4747")
        .header("content-type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let request = request
        .body(
            body.map(|b| Body::from(b.to_string()))
                .unwrap_or_else(Body::empty),
        )
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    (status, value)
}

/// A terminal host served from a thread of the test, with its data in
/// `dir`, and its registration.
pub fn host(dir: &std::path::Path) -> pinrail_host::registration::Registration {
    use std::time::{Duration, Instant};
    let data_dir = dir.join("host-data");
    let config = pinrail_host::server::Config {
        data_dir: data_dir.clone(),
        idle_exit: Duration::from_secs(600),
        ping_every: Duration::from_secs(15),
        // the programs are this process's own, through a holder on a thread
        holder: pinrail_host::holders::Start::Thread,
    };
    // as the host's own process does: on Windows the console hosts its
    // programs run under would otherwise keep the test's output open
    pinrail_host::keep_own_output_from_children();
    std::thread::spawn(move || pinrail_host::server::serve(config));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(registration) =
            pinrail_host::registration::Registration::read(&data_dir.join("host/host.json"))
        {
            return registration;
        }
        assert!(Instant::now() < deadline, "the host did not start");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A stand-in for Claude Code: it writes its arguments and its run token
/// into the folder it starts in, each file whole, then exits with the code in the folder's
/// `exit` file, or waits.
#[cfg(unix)]
pub const STAND_IN_AGENT: &str = r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo "1.0.0 (stand-in)"; exit 0; fi
echo "stand-in agent starting"
# each written whole, then renamed, so a test never reads half of one
printf '%s\n' "$@" > argv.part && mv argv.part argv
printf '%s' "$TERM_PROGRAM" > term_program.part && mv term_program.part term_program
printf '%s' "$PINRAIL_RUN_TOKEN" > token.part && mv token.part token
printf '%s' "$PINRAIL_DATA_DIR" > data_dir.part && mv data_dir.part data_dir
if [ -f exit ]; then exit "$(cat exit)"; fi
# with a file named listen, it writes each line it is sent to heard
if [ -f listen ]; then while IFS= read -r line; do printf '%s\n' "$line" >> heard; done; exit 0; fi
# with a file named records, it writes what a test puts in the file named
# record as an OSC 7501 record, and removes it, and writes each line it is
# sent to heard
if [ -f records ]; then
  exec 3<&0
  (while IFS= read -r line; do printf '%s\n' "$line" >> heard; done) <&3 &
  while :; do
    if [ -f record ]; then printf '\033]7501;%s\033\\' "$(cat record)"; rm record; fi
    if [ -f notify ]; then printf '\033]777;notify;%s\033\\' "$(cat notify)"; rm notify; fi
    sleep 0.05
  done
fi
exec sleep 600
"#;

/// Git's `sh`, which runs the tests' scripts on Windows, as it does CI's
/// steps.
#[cfg(windows)]
pub fn sh() -> std::path::PathBuf {
    let git = std::path::PathBuf::from(r"C:\Program Files\Git\usr\bin\sh.exe");
    if git.is_file() {
        return git;
    }
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join("sh.exe"))
        .find(|sh| sh.is_file())
        .expect("Git's sh.exe, for the tests' scripts")
}

/// The `PATH` that finds the programs in `bin` first, then the system's: on
/// Windows Git's tools too, which the tests' scripts use.
pub fn path_with(bin: &std::path::Path) -> String {
    #[cfg(windows)]
    {
        let system = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        format!(
            r"{};{};{system}\System32;{system}",
            bin.display(),
            sh().parent().unwrap().display()
        )
    }
    #[cfg(unix)]
    {
        format!("{}:/usr/bin:/bin", bin.display())
    }
}

/// Writes `name` in `dir` as a batch file that runs `script` with Git's
/// `sh`, as npm installs agents on Windows.
#[cfg(windows)]
pub fn program(dir: &std::path::Path, name: &str, script: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let file = dir.join(format!("{name}.sh"));
    std::fs::write(&file, format!("{script}\n")).unwrap();
    shim(dir, name, &file);
}

/// `dir/name.cmd`, which runs `script` with Git's `sh` and passes its
/// arguments on.
#[cfg(windows)]
fn shim(dir: &std::path::Path, name: &str, script: &std::path::Path) {
    std::fs::write(
        dir.join(format!("{name}.cmd")),
        format!("@\"{}\" \"{}\" %*\r\n", sh().display(), script.display()),
    )
    .unwrap();
}

/// Links `name` in `dir` to a script that runs `script`, written once for
/// its content under Cargo's target folder: macOS scans a new executable
/// for about 25 seconds the first time it runs, longer than a program has
/// to print its version.
#[cfg(unix)]
pub fn program(dir: &std::path::Path, name: &str, script: &str) {
    use std::hash::{Hash, Hasher};
    use std::os::unix::fs::PermissionsExt;
    let content = format!("#!/bin/sh\n{script}\n");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    let kept = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("programs")
        .join(format!("{:016x}", hasher.finish()));
    if !kept.is_file() {
        std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
        // written beside it and renamed, as other tests may run it meanwhile
        let fresh = kept.with_extension(std::process::id().to_string());
        std::fs::write(&fresh, content).unwrap();
        std::fs::set_permissions(&fresh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::rename(&fresh, &kept).unwrap();
    }
    std::fs::create_dir_all(dir).unwrap();
    std::os::unix::fs::symlink(&kept, dir.join(name)).unwrap();
}

/// Links the stand-in agent as `claude` into `dir/bin`, and returns the
/// `PATH` that finds it.
///
/// The script itself is written once, under Cargo's target folder, and
/// rewritten only when it changes: macOS scans a new executable for about
/// 25 seconds the first time it runs, which a fresh copy for each test
/// would pay every time.
pub fn stand_in_path(dir: &std::path::Path) -> String {
    stand_in_path_as(dir, &["claude"])
}

/// The stand-in agent on Windows, for node: what [`STAND_IN_AGENT`] does.
/// An agent npm installs runs as node does, behind a batch file, so its
/// arguments reach it as Windows quotes them, which Git's `sh` reads
/// otherwise.
#[cfg(windows)]
pub const STAND_IN_AGENT_JS: &str = r#"const fs = require("fs");
const args = process.argv.slice(2);
if (args[0] === "--version") { console.log("1.0.0 (stand-in)"); process.exit(0); }
console.log("stand-in agent starting");
// each written whole, then renamed, so a test never reads half of one
const whole = (name, text) => { fs.writeFileSync(name + ".part", text); fs.renameSync(name + ".part", name); };
// as sh's printf '%s\n' "$@" writes them: one empty line for none
whole("argv", (args.length ? args : [""]).map((a) => a + "\n").join(""));
whole("term_program", process.env.TERM_PROGRAM || "");
whole("token", process.env.PINRAIL_RUN_TOKEN || "");
whole("data_dir", process.env.PINRAIL_DATA_DIR || "");
if (fs.existsSync("exit")) process.exit(Number(fs.readFileSync("exit", "utf8").trim()));
const hear = (then) => {
  let rest = "";
  // raw, as an agent reads its terminal: Windows then passes the escape
  // sequences of a bracketed paste on, which it drops from a line it edits
  if (process.stdin.isTTY) process.stdin.setRawMode(true);
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (chunk) => {
    rest += chunk;
    let at;
    while ((at = rest.search(/\r?\n|\r/)) >= 0) {
      const line = rest.slice(0, at);
      rest = rest.slice(at + (rest.slice(at, at + 2) === "\r\n" ? 2 : 1));
      fs.appendFileSync("heard", line + "\n");
    }
  });
  process.stdin.on("end", then);
};
if (fs.existsSync("listen")) {
  hear(() => process.exit(0));
} else if (fs.existsSync("records")) {
  hear(() => {});
  setInterval(() => {
    if (fs.existsSync("record")) { process.stdout.write("\x1b]7501;" + fs.readFileSync("record", "utf8") + "\x1b\\"); fs.rmSync("record"); }
    if (fs.existsSync("notify")) { process.stdout.write("\x1b]777;notify;" + fs.readFileSync("notify", "utf8") + "\x1b\\"); fs.rmSync("notify"); }
  }, 50);
} else {
  setTimeout(() => {}, 600 * 1000);
}
"#;

/// The node that runs the stand-in agent on Windows: the one the project's
/// tools give, by its own path, since a version manager's shim needs the
/// project's folder.
#[cfg(windows)]
pub fn node() -> std::path::PathBuf {
    static NODE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    NODE.get_or_init(|| {
        let output = std::process::Command::new("node")
            .args(["-p", "process.execPath"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("node, for the stand-in agent");
        std::path::PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
    })
    .clone()
}

/// Puts the stand-in agent as each of `names` into `dir/bin`, a batch file
/// that runs it with node, and returns the `PATH` that finds them.
#[cfg(windows)]
pub fn stand_in_path_as(dir: &std::path::Path, names: &[&str]) -> String {
    let script = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("stand-in-claude.cjs");
    if std::fs::read_to_string(&script).ok().as_deref() != Some(STAND_IN_AGENT_JS) {
        let fresh = script.with_extension(format!("{}", std::process::id()));
        std::fs::write(&fresh, STAND_IN_AGENT_JS).unwrap();
        std::fs::rename(&fresh, &script).unwrap();
    }
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for name in names {
        std::fs::write(
            bin.join(format!("{name}.cmd")),
            format!("@\"{}\" \"{}\" %*\r\n", node().display(), script.display()),
        )
        .unwrap();
    }
    path_with(&bin)
}

/// Links the stand-in agent as each of `names` into `dir/bin`, and returns
/// the `PATH` that finds them.
#[cfg(unix)]
pub fn stand_in_path_as(dir: &std::path::Path, names: &[&str]) -> String {
    use std::os::unix::fs::PermissionsExt;
    let script = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("stand-in-claude");
    if std::fs::read_to_string(&script).ok().as_deref() != Some(STAND_IN_AGENT) {
        // written beside it and renamed, as other tests may run it meanwhile
        let fresh = script.with_extension(format!("{}", std::process::id()));
        std::fs::write(&fresh, STAND_IN_AGENT).unwrap();
        std::fs::set_permissions(&fresh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::rename(&fresh, &script).unwrap();
    }
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for name in names {
        std::os::unix::fs::symlink(&script, bin.join(name)).unwrap();
    }
    path_with(&bin)
}

/// Has the stand-in agent in `folder` write `record` as an OSC 7501 record,
/// and waits until it has.
/// Has the stand-in agent in `folder` write `notify` (a title, `;`, a body)
/// as an OSC 777 notification, and waits until it has.
pub async fn notify(folder: &std::path::Path, notify: &str) {
    std::fs::write(folder.join("notify.part"), notify).unwrap();
    std::fs::rename(folder.join("notify.part"), folder.join("notify")).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while folder.join("notify").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the notification was not written"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

pub async fn record(folder: &std::path::Path, record: &str) {
    std::fs::write(folder.join("record.part"), record).unwrap();
    std::fs::rename(folder.join("record.part"), folder.join("record")).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while folder.join("record").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the record was not written"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// The rule that lets Claude Code read `folder`, as Pinrail gives it: an
/// absolute path starts with `//`, and on Windows is in POSIX form, as
/// Claude Code matches paths there, `C:\\Users` as `/c/Users`.
pub fn read_rule(folder: &std::path::Path) -> String {
    let folder = folder.to_string_lossy();
    let folder = match folder.as_bytes() {
        [drive, b':', ..] if drive.is_ascii_alphabetic() => format!(
            "/{}{}",
            (*drive as char).to_ascii_lowercase(),
            folder[2..].replace('\\', "/")
        ),
        _ => folder.into_owned(),
    };
    format!("Read(/{folder}/**)")
}
