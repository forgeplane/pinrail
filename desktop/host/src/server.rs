//! `pinrail-host serve`: takes the lock, listens on the socket, writes the
//! registration, and answers one controller at a time until it is stopped
//! or has been idle too long.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::net::UnixStream;
use tokio::net::unix::OwnedWriteHalf;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{Notify, mpsc, oneshot};

use crate::environment;
use crate::frame::{self, CONTROL_CHANNEL, Frame, kind};
use crate::log::Log;
use crate::paths::Paths;
use crate::pty::{self, SpawnError};
use crate::registration::Registration;
use crate::terminal::{self, Terminal};

/// How long a host with no terminals and no controller waits before it
/// exits.
pub const IDLE_EXIT: Duration = Duration::from_secs(10 * 60);
/// The most live terminals a host runs.
pub const MAX_TERMINALS: usize = 64;
/// The lines of history each terminal keeps.
pub const SCROLLBACK_LINES: usize = 10_000;
/// How long a connection has to send its `hello`.
const HELLO_WITHIN: Duration = Duration::from_secs(10);

pub struct Config {
    pub data_dir: PathBuf,
    pub idle_exit: Duration,
}

#[derive(Debug)]
pub enum Error {
    /// Another host holds the lock; the process it named, if it did.
    Running(Option<u32>),
    /// The system did not give a boot id, without which a registration
    /// cannot be told from one of an earlier boot.
    NoBootId,
    Io(io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Running(Some(pid)) => write!(f, "another host is running (process {pid})"),
            Error::Running(None) => write!(f, "another host is running"),
            Error::NoBootId => write!(f, "the system did not give a boot id"),
            Error::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::Io(error)
    }
}

/// Runs the host until it is stopped, by `host.stop`, a signal, or being
/// idle for `idle_exit`.
pub fn serve(config: Config) -> Result<(), Error> {
    let paths = Paths::new(&config.data_dir);
    paths.create()?;
    let _lock = take_lock(&paths)?;
    let boot_id = crate::boot_id().ok_or(Error::NoBootId)?;
    let log = Log::open(&paths.log());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(run(&paths, boot_id, &log, config.idle_exit));
    let _ = fs::remove_file(paths.registration());
    let _ = fs::remove_file(paths.socket());
    match &result {
        Ok(reason) => log.line(&format!("stopped: {reason}")),
        Err(error) => log.line(&format!("failed: {error}")),
    }
    result.map(|_| ()).map_err(Error::Io)
}

/// Takes `host.lock` and writes this process's id into it. The system
/// releases the lock when the process ends, however it ends.
fn take_lock(paths: &Paths) -> Result<File, Error> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(paths.lock())?;
    fs::set_permissions(paths.lock(), fs::Permissions::from_mode(0o600))?;
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            let pid = fs::read_to_string(paths.lock())
                .ok()
                .and_then(|s| s.trim().parse().ok());
            return Err(Error::Running(pid));
        }
        Err(TryLockError::Error(error)) => return Err(error.into()),
    }
    file.set_len(0)?;
    file.write_all(std::process::id().to_string().as_bytes())?;
    Ok(file)
}

/// What every connection shares.
struct Shared {
    secret: String,
    instance: String,
    boot_id: String,
    started_at: String,
    /// The connection that is the controller, and how to tell it that it
    /// was replaced.
    controller: Mutex<Option<(u64, oneshot::Sender<()>)>>,
    /// Since when the host has had no controller.
    idle_since: Mutex<Option<Instant>>,
    stop: Notify,
    terminals: Mutex<BTreeMap<String, Terminal>>,
}

async fn run(paths: &Paths, boot_id: String, log: &Log, idle_exit: Duration) -> io::Result<String> {
    let socket = paths.socket();
    // the lock is ours, so a socket left here is from a host that ended
    let _ = fs::remove_file(&socket);
    let listener = tokio::net::UnixListener::bind(&socket)?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
    let shared = Arc::new(Shared {
        secret: crate::random_hex(32),
        instance: crate::random_hex(8),
        boot_id,
        started_at: crate::now(),
        controller: Mutex::new(None),
        idle_since: Mutex::new(Some(Instant::now())),
        stop: Notify::new(),
        terminals: Mutex::new(BTreeMap::new()),
    });
    Registration {
        pid: std::process::id(),
        started_at: shared.started_at.clone(),
        boot_id: shared.boot_id.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        protocol: crate::GENERATION,
        features: vec![],
        socket: socket.clone(),
        secret: shared.secret.clone(),
    }
    .write(&paths.registration())?;
    log.line(&format!(
        "started: process {}, socket {}",
        std::process::id(),
        socket.display()
    ));
    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut tick = tokio::time::interval(idle_exit.min(Duration::from_secs(1)));
    let mut next_connection = 0u64;
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                if let Ok((stream, _)) = accepted {
                    next_connection += 1;
                    tokio::spawn(connection(stream, next_connection, shared.clone()));
                }
            }
            () = shared.stop.notified() => return Ok("host.stop".to_string()),
            _ = terminate.recv() => return Ok("SIGTERM".to_string()),
            _ = interrupt.recv() => return Ok("SIGINT".to_string()),
            _ = tick.tick() => {
                let idle = *shared.idle_since.lock().unwrap_or_else(|e| e.into_inner());
                if shared.live_terminals() == 0
                    && idle.is_some_and(|since| since.elapsed() >= idle_exit)
                {
                    return Ok("idle".to_string());
                }
            }
        }
    }
}

/// One connection: its `hello`, then JSON-RPC calls on channel 0 until it
/// closes, is replaced, or stops the host.
async fn connection(stream: UnixStream, id: u64, shared: Arc<Shared>) {
    let (mut reader, writer) = stream.into_split();
    let Ok(Ok(Some(first))) = tokio::time::timeout(HELLO_WITHIN, frame::read(&mut reader)).await
    else {
        return;
    };
    let Some(call_id) = hello_id(&first, &shared.secret) else {
        return;
    };
    let (out, queue) = mpsc::unbounded_channel();
    let writer = tokio::spawn(write_queue(writer, queue));
    let (replace, mut replaced) = oneshot::channel();
    if let Some((_, previous)) = shared
        .controller
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace((id, replace))
    {
        let _ = previous.send(());
    }
    *shared.idle_since.lock().unwrap_or_else(|e| e.into_inner()) = None;
    let _ = out.send(Frame::json(&result(call_id, hello_result(&shared))));
    let mut stop = false;
    loop {
        tokio::select! {
            _ = &mut replaced => {
                let _ = out.send(Frame {
                    channel: CONTROL_CHANNEL,
                    kind: kind::CLOSE,
                    payload: json!({"reason": "replaced"}).to_string().into_bytes(),
                });
                break;
            }
            read = frame::read(&mut reader) => match read {
                Ok(Some(frame)) if frame.channel == CONTROL_CHANNEL && frame.kind == kind::DATA => {
                    let (reply, stops) = answer(&frame.payload, &shared).await;
                    if let Some(reply) = reply {
                        let _ = out.send(Frame::json(&reply));
                    }
                    if stops {
                        stop = true;
                        break;
                    }
                }
                // there are no terminal channels yet
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }
    }
    drop(out);
    let _ = writer.await;
    {
        let mut controller = shared.controller.lock().unwrap_or_else(|e| e.into_inner());
        if controller.as_ref().is_some_and(|(c, _)| *c == id) {
            *controller = None;
            *shared.idle_since.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        }
    }
    if stop {
        shared.stop.notify_one();
    }
}

/// Writes the queued frames in order, the connection's only writer.
async fn write_queue(mut writer: OwnedWriteHalf, mut queue: mpsc::UnboundedReceiver<Frame>) {
    while let Some(frame) = queue.recv().await {
        if frame::write(&mut writer, &frame).await.is_err() {
            break;
        }
    }
}

/// The id of a `hello` call on channel 0 with the right secret.
fn hello_id(frame: &Frame, secret: &str) -> Option<Value> {
    if frame.channel != CONTROL_CHANNEL || frame.kind != kind::DATA {
        return None;
    }
    let message: Value = serde_json::from_slice(&frame.payload).ok()?;
    if message.get("method")?.as_str()? != "hello" {
        return None;
    }
    let given = message.get("params")?.get("secret")?.as_str()?;
    same(given.as_bytes(), secret.as_bytes()).then(|| message.get("id").cloned())?
}

/// Compares in time that does not depend on where the inputs differ.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn hello_result(shared: &Shared) -> Value {
    json!({
        "host": crate::name(),
        "generation": crate::GENERATION,
        "boot_id": shared.boot_id,
        "features": [],
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "home": std::env::var("HOME").unwrap_or_default(),
        "user": std::env::var("USER").unwrap_or_default(),
        "instance": shared.instance,
        "time": crate::now(),
    })
}

/// The reply to a JSON-RPC message, if it is a call, and whether the host
/// stops after sending it.
async fn answer(payload: &[u8], shared: &Shared) -> (Option<Value>, bool) {
    let Ok(message) = serde_json::from_slice::<Value>(payload) else {
        return (
            Some(error(Value::Null, -32700, "parse_error", "not JSON")),
            false,
        );
    };
    // a notification has no id and gets no reply
    let Some(id) = message.get("id").cloned() else {
        return (None, false);
    };
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "host.info" => (
            Some(result(
                id,
                json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "generation": crate::GENERATION,
                    "features": [],
                    "boot_id": shared.boot_id,
                    "started_at": shared.started_at,
                    "terminals": shared.live_terminals(),
                    "os": std::env::consts::OS,
                    "arch": std::env::consts::ARCH,
                }),
            )),
            false,
        ),
        "host.stop" => (Some(result(id, json!({}))), true),
        "terminal.spawn" => (Some(reply(id, spawn(params(&message), shared))), false),
        "terminal.list" => {
            let list: Vec<Value> = shared
                .terminals
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .values()
                .map(|t| t.info().to_json())
                .collect();
            (Some(result(id, Value::Array(list))), false)
        }
        "terminal.text" => (Some(reply(id, text(params(&message), shared).await)), false),
        _ => (
            Some(error(
                id,
                -32601,
                "unknown_method",
                &format!("no method {method:?}"),
            )),
            false,
        ),
    }
}

/// A call's params, or an empty object.
fn params(message: &Value) -> &Value {
    static EMPTY: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| json!({}));
    message.get("params").unwrap_or(&EMPTY)
}

/// A method's failure: its `data.code` and message.
struct Failure(&'static str, String);

fn reply(id: Value, outcome: Result<Value, Failure>) -> Value {
    match outcome {
        Ok(value) => result(id, value),
        Err(Failure(code, message)) => error(id, -32000, code, &message),
    }
}

impl Shared {
    fn live_terminals(&self) -> usize {
        self.terminals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|t| t.live())
            .count()
    }
}

/// `terminal.spawn` (section 4.3 of the runtime specification).
fn spawn(params: &Value, shared: &Shared) -> Result<Value, Failure> {
    let invalid = |what: &str| Failure("invalid_params", format!("{what} is missing or invalid"));
    let argv: Vec<String> = params
        .get("argv")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .filter(|a: &Vec<String>| !a.is_empty())
        .ok_or_else(|| invalid("argv"))?;
    let cwd = params
        .get("cwd")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("cwd"))?;
    let size = |key| {
        params
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|n| u16::try_from(n).ok())
            .filter(|&n| n > 0)
    };
    let (cols, rows) = (
        size("cols").ok_or_else(|| invalid("cols"))?,
        size("rows").ok_or_else(|| invalid("rows"))?,
    );
    let strings = |key| -> BTreeMap<String, String> {
        params
            .get(key)
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default()
    };
    let labels = params.get("labels").cloned().unwrap_or_else(|| json!({}));
    if shared.live_terminals() >= MAX_TERMINALS {
        return Err(Failure(
            "limit",
            format!("the host runs {MAX_TERMINALS} terminals already"),
        ));
    }
    let id = format!("t_{}", crate::random_hex(8));
    // the login environment comes with the probe; until then the host's own
    let base: Vec<(String, String)> = std::env::vars().collect();
    let env = environment::build(&environment::Request {
        base: &base,
        env: &strings("env"),
        session_env: &strings("session_env"),
        cwd,
        terminal: &id,
    });
    let spawned = pty::spawn(&argv, Path::new(cwd), &env, cols, rows).map_err(|e| match e {
        SpawnError::FolderMissing => Failure("folder_missing", format!("{cwd} does not exist")),
        SpawnError::NotFound(name) => Failure("not_found", format!("{name} was not found")),
        SpawnError::Io(error) => Failure("spawn_failed", error.to_string()),
    })?;
    let screen = terminal::screen(cols, rows, SCROLLBACK_LINES)
        .map_err(|e| Failure("spawn_failed", e.to_string()))?;
    let pid = spawned.pid;
    let terminal = Terminal::start(id.clone(), spawned, labels, cols, rows, screen)
        .map_err(|e| Failure("spawn_failed", e.to_string()))?;
    let started_at = terminal.info().started_at;
    shared
        .terminals
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id.clone(), terminal);
    Ok(json!({"terminal": id, "pid": pid, "started_at": started_at}))
}

/// `terminal.text`: the screen after up to `lines` lines of history.
async fn text(params: &Value, shared: &Shared) -> Result<Value, Failure> {
    let id = params.get("terminal").and_then(Value::as_str).unwrap_or("");
    let lines = params.get("lines").and_then(Value::as_u64).unwrap_or(0) as usize;
    let request = {
        let terminals = shared.terminals.lock().unwrap_or_else(|e| e.into_inner());
        let terminal = terminals
            .get(id)
            .ok_or_else(|| Failure("not_found", format!("no terminal {id:?}")))?;
        terminal.text_request(lines)
    };
    let text = request
        .await
        .ok_or_else(|| Failure("not_found", format!("no terminal {id:?}")))?;
    Ok(json!({"text": text}))
}

fn result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error(id: Value, code: i64, data_code: &str, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message, "data": {"code": data_code}},
    })
}
