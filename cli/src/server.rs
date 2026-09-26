//! Finding and starting the server.
//!
//! The running server writes `<data dir>/server.json`; that file, after
//! `PINRAIL_URL` and `--url`, is how the CLI finds it, and `PINRAIL_PORT`
//! decides the default when nothing is advertised. Starting one needs a
//! command, `PINRAIL_SERVER_CMD`, run through `sh -c`: usually the desktop
//! app's binary with `--headless`.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::api::Client;

pub const DEFAULT_PORT: &str = "4747";

/// The URL to try when nothing is advertised: the loopback address on
/// `PINRAIL_PORT`, the same variable the server reads, else 4747.
pub fn default_url() -> String {
    let port = std::env::var("PINRAIL_PORT").unwrap_or_else(|_| DEFAULT_PORT.to_string());
    format!("http://127.0.0.1:{port}")
}

pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("PINRAIL_DATA_DIR") {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("pinrail")
}

pub fn info_path() -> PathBuf {
    data_dir().join("server.json")
}

/// What the running server advertised, if anything.
pub fn advertised() -> Option<Value> {
    let text = std::fs::read_to_string(info_path()).ok()?;
    serde_json::from_str(&text).ok()
}

/// The base URL to talk to. With `auto_start`, a server that is not
/// answering is started first (create does this; nothing else).
pub fn resolve_url(explicit: Option<&str>, auto_start: bool) -> Result<String> {
    if let Some(url) = explicit {
        return Ok(url.trim_end_matches('/').to_string());
    }
    let url = advertised()
        .and_then(|v| v["url"].as_str().map(str::to_string))
        .unwrap_or_else(default_url);

    if auto_start && !Client::new(&url).reachable() {
        crate::out::note(format_args!("server not running at {url}, starting it"));
        let info = start()?;
        return Ok(info["url"].as_str().unwrap_or(&url).to_string());
    }
    Ok(url)
}

/// `pinrail serve`: the running server's info, starting one if needed.
pub fn ensure_running(explicit: Option<&str>) -> Result<Value> {
    let url = resolve_url(explicit, false)?;
    if Client::new(&url).reachable() {
        return Ok(advertised().unwrap_or_else(|| json!({ "url": url })));
    }
    start()
}

fn start() -> Result<Value> {
    let Ok(command) = std::env::var("PINRAIL_SERVER_CMD") else {
        bail!(
            "the server is not running; open the Pinrail app, or set PINRAIL_SERVER_CMD \
             to a command that starts it (the app's binary with --headless) and retry"
        );
    };

    let dir = data_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let log = std::fs::File::create(dir.join("server.log")).context("opening server.log")?;
    let _ = std::fs::remove_file(info_path());

    let mut cmd = Command::new("sh");
    cmd.args(["-c", &command])
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .env("PINRAIL_DATA_DIR", &dir);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // its own process group: it outlives this CLI invocation
        cmd.process_group(0);
    }
    let child = cmd
        .spawn()
        .with_context(|| format!("starting the server with {command}"))?;
    crate::out::note(format_args!(
        "started server (pid {}), log at {}",
        child.id(),
        dir.join("server.log").display()
    ));

    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(info) = advertised()
            && let Some(url) = info["url"].as_str()
            && Client::new(url).reachable()
        {
            return Ok(info);
        }
        if Instant::now() > deadline {
            bail!(
                "the server did not come up within 60s; see {}",
                dir.join("server.log").display()
            );
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

pub fn open_browser(url: &str) -> Result<()> {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let status = Command::new(program)
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("running {program}"))?;
    if !status.success() {
        bail!("{program} exited with {status}");
    }
    Ok(())
}
