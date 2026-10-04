//! Finding and starting the server.
//!
//! The running server writes `<data dir>/server.json`; that file, after
//! `PINRAIL_URL` and `--url`, is how the CLI finds it, and `PINRAIL_PORT`
//! decides the default when nothing is advertised. Starting one needs a
//! command, `PINRAIL_SERVER_CMD`, run through `sh -c`: usually the desktop
//! app's binary with `--headless`.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
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

/// Where the address the CLI talks to came from, set once it is chosen.
static SOURCE: OnceLock<Source> = OnceLock::new();

#[derive(Clone, Copy, PartialEq)]
enum Source {
    Given,
    Advertised,
    Default,
}

/// What to say when nothing answers at `url`: the address, where it came
/// from, and how to get a server there.
pub fn not_answering(url: &str) -> String {
    match SOURCE.get().copied().unwrap_or(Source::Default) {
        Source::Given => format!(
            "the server is not answering at {url} (from --url or PINRAIL_URL); \
             open the Pinrail app there, or correct the address, and retry"
        ),
        source => format!(
            "the server is not answering at {url} ({}); open the Pinrail app, or set \
             PINRAIL_SERVER_CMD to a command that starts it (the app's binary with \
             --headless), and retry",
            if source == Source::Advertised {
                format!("from {}", info_path().display())
            } else {
                "the default".to_string()
            }
        ),
    }
}

/// What the running server advertised, if anything.
pub fn advertised() -> Option<Value> {
    let text = std::fs::read_to_string(info_path()).ok()?;
    serde_json::from_str(&text).ok()
}

/// The base URL to talk to. With `auto_start`, a server that is not
/// answering is started first, when PINRAIL_SERVER_CMD says how. `submit`,
/// `plugins`, `plugins describe` and `plugins new --link` ask for it;
/// `plugins check` reads the folder alone and needs no server.
pub fn resolve_url(explicit: Option<&str>, auto_start: bool) -> Result<String> {
    if let Some(url) = explicit {
        let _ = SOURCE.set(Source::Given);
        return Ok(url.trim_end_matches('/').to_string());
    }
    let (url, source) = match advertised().and_then(|v| v["url"].as_str().map(str::to_string)) {
        Some(url) => (url, Source::Advertised),
        None => (default_url(), Source::Default),
    };
    let _ = SOURCE.set(source);

    if auto_start && !Client::new(&url).reachable() {
        crate::out::note(format_args!("server not running at {url}, starting it"));
        let info = start(&url)?;
        return Ok(info["url"].as_str().unwrap_or(&url).to_string());
    }
    Ok(url)
}

/// `pinrail serve`: the running server's info, starting one if needed.
pub fn ensure_running(explicit: Option<&str>) -> Result<Value> {
    let url = resolve_url(explicit, false)?;
    if Client::new(&url).reachable() {
        // what server.json says, when it is about this server
        return Ok(advertised()
            .filter(|info| {
                info["url"].as_str().map(|u| u.trim_end_matches('/')) == Some(url.as_str())
            })
            .unwrap_or_else(|| json!({ "url": url })));
    }
    start(&url)
}

fn start(url: &str) -> Result<Value> {
    let Ok(command) = std::env::var("PINRAIL_SERVER_CMD") else {
        bail!("{}", not_answering(url));
    };

    let dir = data_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let log_path = dir.join("server.log");
    // appended to, and server.json left alone: another agent may have just
    // started a server that is writing to both
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .context("opening server.log")?;

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
    let mut child = cmd
        .spawn()
        .with_context(|| format!("starting the server with {command}"))?;
    crate::out::note(format_args!(
        "started server (pid {}), log at {}",
        child.id(),
        log_path.display()
    ));

    // Any server that answers counts, this one or one another start brought
    // up at the same moment: then this start's own server, which cannot
    // take the port, simply exits.
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(info) = advertised()
            && let Some(advertised) = info["url"].as_str()
            && Client::new(advertised).reachable()
        {
            return Ok(info);
        }
        if Client::new(url).reachable() {
            return Ok(json!({ "url": url }));
        }
        if let Ok(Some(status)) = child.try_wait() {
            bail!(
                "the server command exited ({status}) and no server is answering; see {}",
                log_path.display()
            );
        }
        if Instant::now() > deadline {
            bail!(
                "the server did not come up within 60s; see {}",
                log_path.display()
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
