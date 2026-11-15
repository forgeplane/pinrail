//! `pinrail-host serve`: takes the lock, listens on the socket, writes the
//! registration, and answers one controller at a time until it is stopped
//! or has been idle too long.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::net::UnixStream;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{Notify, mpsc, oneshot};

use crate::environment;
use crate::events::Events;
use crate::frame::{self, CONTROL_CHANNEL, Frame, kind};
use crate::log::Log;
use crate::outbox::Outbox;
use crate::paths::Paths;
use crate::probe::Probe;
use crate::pty::SpawnError;
use crate::records::Records;
use crate::registration::Registration;
use crate::terminal::{self, AttachError, Reason, SignalError, Terminal};
use crate::viewers::AttachRequest;

/// How long a host with no terminals and no controller waits before it
/// exits.
pub const IDLE_EXIT: Duration = Duration::from_secs(10 * 60);
/// Ended terminals kept, with their final screens, for at most 24 hours.
const KEPT_ENDED: usize = 200;
/// The most live terminals a host runs.
pub const MAX_TERMINALS: usize = 64;
/// The lines of history each terminal keeps.
pub const SCROLLBACK_LINES: usize = 10_000;
/// How long a connection has to send its `hello`.
const HELLO_WITHIN: Duration = Duration::from_secs(10);

pub struct Config {
    pub data_dir: PathBuf,
    pub idle_exit: Duration,
    /// How often the controller hears from an idle host.
    pub ping_every: Duration,
    /// How to start a holder when none runs.
    pub holder: crate::holders::Start,
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
    crate::sys::raise_open_files(8_192);
    let boot_id = crate::boot_id().ok_or(Error::NoBootId)?;
    let log = Log::open(&paths.log());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(run(
        &paths,
        boot_id,
        &log,
        config.idle_exit,
        config.ping_every,
        config.holder,
    ));
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
    ping_every: Duration,
    terminals: Mutex<BTreeMap<String, Terminal>>,
    context: terminal::Context,
    probe: Probe,
    /// The holders that keep the terminals' PTYs open.
    holders: Arc<crate::holders::Holders>,
    /// Where a host that hands its terminals over writes their screens.
    hand_over_dir: PathBuf,
    /// `host.log`, for what adoption found.
    log: Log,
}

/// The snapshot formats this host reads, best first: the binary snapshot
/// of its own libghostty-vt build, then the `vt` paint any build reads.
/// The launcher asks a new host for them before the old one hands over.
pub fn snapshot_formats() -> Vec<String> {
    vec![
        format!("ghostty-bin;vt-{}", pinrail_screen::ghostty::version()),
        "vt".to_string(),
    ]
}

async fn run(
    paths: &Paths,
    boot_id: String,
    log: &Log,
    idle_exit: Duration,
    ping_every: Duration,
    holder: crate::holders::Start,
) -> io::Result<String> {
    let socket = paths.socket();
    let (exits, mut exited) = tokio::sync::mpsc::unbounded_channel();
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
        ping_every,
        terminals: Mutex::new(BTreeMap::new()),
        probe: Probe::default(),
        context: terminal::Context {
            events: Arc::new(Events::default()),
            records: Arc::new(Records::open(paths.host_dir.join("terminals.json"))),
            screens: paths.host_dir.join("screens"),
            controlled: Arc::new(AtomicBool::new(false)),
        },
        holders: Arc::new(crate::holders::Holders::new(paths.clone(), holder, exits)),
        hand_over_dir: paths.host_dir.join("handover"),
        log: Log::open(&paths.log()),
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
        build: std::env::current_exe()
            .ok()
            .and_then(|exe| crate::launch::build_of(&exe).ok()),
    }
    .write(&paths.registration())?;
    log.line(&format!(
        "started: process {}, socket {}",
        std::process::id(),
        socket.display()
    ));
    let records = shared.context.records.clone();
    tokio::spawn(async move { records.writer().await });
    // the terminals the holders kept while no host ran
    let adopting = shared.clone();
    tokio::task::spawn_blocking(move || adopt(&adopting))
        .await
        .map_err(io::Error::other)?;
    shared.forget_earlier();
    // the exits the holders report, to their terminals
    let exits_to = shared.clone();
    tokio::spawn(async move {
        while let Some((_, terminal, exit)) = exited.recv().await {
            let terminals = exits_to.terminals.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(terminal) = terminals.get(&terminal) {
                terminal.exited(exit);
            }
        }
    });
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
                shared.prune();
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

/// Takes back the terminals the holders keep. A terminal with a record
/// gets a screen at the PTY's size, rebuilt from its output ring with
/// replies and signals discarded, since they are old; the program is then
/// asked to redraw, as the ring may begin in the middle of a screen it has
/// since changed. A terminal without a record, or whose record says it
/// ended, belongs to no host any more: it is stopped and forgotten.
fn adopt(shared: &Arc<Shared>) {
    for kept in shared.holders.adopt() {
        let crate::holders::Kept {
            holder,
            terminal: id,
            spawned,
            ring,
            exit,
        } = kept;
        let record = shared
            .context
            .records
            .get(&id)
            .filter(|r| r["ended_at"].is_null());
        let Some(record) = record else {
            let _ = crate::pty::signal_group(spawned.pid, libc::SIGKILL);
            shared.holders.forget(&holder, &id);
            continue;
        };
        let (cols, rows) = crate::pty::size(&spawned.master).unwrap_or((80, 24));
        let ring = ring.and_then(|fd| crate::ring::SharedRing::open(fd).ok());
        // after an update, the screen the previous host handed over;
        // after a crash, the output ring
        let handed = read_hand_over(&shared.hand_over_dir.join(format!("{id}.handover")));
        let restored = handed
            .as_ref()
            .and_then(|h| restore(h, (cols, rows)).map(|screen| (screen, h)));
        let (screen, last_input_at, held) = match restored {
            Some((screen, h)) => {
                shared
                    .log
                    .line(&format!("adopted {id} from a hand-over in {}", h.format));
                (screen, h.last_input_at.clone(), h.held.clone())
            }
            None => {
                let Ok(mut screen) = terminal::screen(cols, rows, SCROLLBACK_LINES) else {
                    continue;
                };
                if let Some(ring) = &ring {
                    let (mut replies, mut signals) = (Vec::new(), Vec::new());
                    screen.feed(&ring.read(), &mut replies, &mut signals);
                }
                // a different size, then the size again: the program redraws
                let _ = crate::pty::resize(&spawned.master, cols.saturating_sub(1).max(1), rows);
                let _ = crate::pty::resize(&spawned.master, cols, rows);
                (screen, None, Vec::new())
            }
        };
        let labels = record["labels"].clone();
        let forget = {
            let (holders, holder, id) = (shared.holders.clone(), holder.clone(), id.clone());
            Box::new(move || holders.forget(&holder, &id))
        };
        let Ok(terminal) = Terminal::start(
            id.clone(),
            spawned,
            labels,
            (cols, rows),
            screen,
            shared.context.clone(),
            terminal::Holding { forget, ring, held },
        ) else {
            continue;
        };
        terminal.adopted_with(last_input_at);
        if let Some(exit) = exit {
            terminal.exited(exit);
        }
        shared
            .terminals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, terminal);
    }
}

/// A screen a previous host handed over.
struct HandedOver {
    format: String,
    last_input_at: Option<String>,
    held: Vec<u8>,
    bytes: Vec<u8>,
}

/// Reads a hand-over file and deletes it before anything uses it, so that
/// a host that crashes while it restores the screen never leaves a stale
/// one for the next.
fn read_hand_over(file: &Path) -> Option<HandedOver> {
    let content = fs::read(file).ok()?;
    let _ = fs::remove_file(file);
    let newline = content.iter().position(|&b| b == b'\n')?;
    let header: Value = serde_json::from_slice(&content[..newline]).ok()?;
    Some(HandedOver {
        format: header["format"].as_str()?.to_string(),
        last_input_at: header["last_input_at"].as_str().map(str::to_string),
        held: header["held"]
            .as_array()
            .map(|b| {
                b.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u8))
                    .collect()
            })
            .unwrap_or_default(),
        bytes: content[newline + 1..].to_vec(),
    })
}

/// The screen a hand-over describes, with replies and signals discarded,
/// or `None` when it cannot be restored and the ring has to do.
fn restore(
    handed: &HandedOver,
    (cols, rows): (u16, u16),
) -> Option<Box<dyn pinrail_screen::Screen>> {
    if handed.format == "vt" {
        let mut screen = terminal::screen(cols, rows, SCROLLBACK_LINES).ok()?;
        let (mut replies, mut signals) = (Vec::new(), Vec::new());
        screen.feed(&handed.bytes, &mut replies, &mut signals);
        return Some(screen);
    }
    let screen =
        pinrail_screen::ghostty::Terminal::restore(&handed.bytes, SCROLLBACK_LINES).ok()?;
    Some(Box::new(screen))
}

/// How long a closing connection's queued frames get to go out.
const FLUSH_WITHIN: Duration = Duration::from_secs(5);
/// How often the controller hears from an idle host.
pub const PING_EVERY: Duration = Duration::from_secs(15);

/// One connection: its `hello`, then JSON-RPC calls on channel 0, each
/// answered by a task of its own, until it closes, is replaced, overflows
/// or stops the host.
async fn connection(stream: UnixStream, id: u64, shared: Arc<Shared>) {
    let (reader, writer) = stream.into_split();
    // the loop below reads in a select!, where a read can be dropped
    let mut reader = frame::FrameReader::new(reader);
    let Ok(Ok(Some(first))) = tokio::time::timeout(HELLO_WITHIN, reader.next()).await else {
        return;
    };
    let Some(call_id) = hello_id(&first, &shared.secret) else {
        return;
    };
    let (out, mut writer) = Outbox::new(writer);
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
    shared.context.controlled.store(true, Ordering::Relaxed);
    out.reply(&result(call_id, hello_result(&shared)));
    let (stopping, mut stop_asked) = mpsc::unbounded_channel::<()>();
    let mut ping = tokio::time::interval_at(
        tokio::time::Instant::now() + shared.ping_every,
        shared.ping_every,
    );
    let mut stop = false;
    let mut overflowed = false;
    // this connection's terminal channels, and the terminal of each
    let channels: Arc<Mutex<BTreeMap<u32, String>>> = Arc::default();
    let next_channel = Arc::new(std::sync::atomic::AtomicU32::new(1));
    loop {
        tokio::select! {
            _ = &mut replaced => {
                out.frame(Frame {
                    channel: CONTROL_CHANNEL,
                    kind: kind::CLOSE,
                    payload: json!({"reason": "replaced"}).to_string().into_bytes(),
                });
                break;
            }
            () = out.overflowed() => {
                overflowed = true;
                break;
            }
            Some(()) = stop_asked.recv() => {
                stop = true;
                break;
            }
            _ = ping.tick() => {
                out.frame(Frame::json(&json!({"jsonrpc": "2.0", "method": "ping"})));
            }
            read = reader.next() => match read {
                Ok(Some(frame)) if frame.channel == CONTROL_CHANNEL && frame.kind == kind::DATA => {
                    // answered in order with the events that follow it
                    if subscribe(&frame.payload, id, &out, &shared) {
                        continue;
                    }
                    if attach(&frame.payload, id, &out, &shared, &channels, &next_channel) {
                        continue;
                    }
                    let (out, shared, stopping) = (out.clone(), shared.clone(), stopping.clone());
                    tokio::spawn(async move {
                        let (reply, stops) = answer(&frame.payload, &shared).await;
                        if let Some(reply) = reply {
                            out.reply(&reply);
                        }
                        if stops {
                            let _ = stopping.send(());
                        }
                    });
                }
                // a frame on a terminal channel, for its terminal's actor
                Ok(Some(frame)) => {
                    let terminal = channels
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .get(&frame.channel)
                        .cloned();
                    if let Some(terminal) = terminal {
                        let channel = frame.channel;
                        let closing = frame.kind == kind::CLOSE;
                        if let Some(t) = shared.terminals.lock().unwrap_or_else(|e| e.into_inner()).get(&terminal) {
                            t.channel((id, channel), frame);
                        }
                        if closing {
                            channels.lock().unwrap_or_else(|e| e.into_inner()).remove(&channel);
                        }
                    }
                }
                Ok(None) | Err(_) => break,
            }
        }
    }
    shared.context.events.unsubscribe(id);
    {
        let terminals = shared.terminals.lock().unwrap_or_else(|e| e.into_inner());
        for (channel, terminal) in channels.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            if let Some(t) = terminals.get(terminal) {
                t.detach((id, *channel));
            }
        }
    }
    drop(out);
    // a connection that reads nothing is closed at once; otherwise what is
    // queued, such as a stop's reply, gets a few seconds to go out
    let flushed = !overflowed
        && tokio::time::timeout(FLUSH_WITHIN, &mut writer)
            .await
            .is_ok();
    if !flushed {
        writer.abort();
    }
    {
        let mut controller = shared.controller.lock().unwrap_or_else(|e| e.into_inner());
        if controller.as_ref().is_some_and(|(c, _)| *c == id) {
            *controller = None;
            *shared.idle_since.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
            shared.context.controlled.store(false, Ordering::Relaxed);
        }
    }
    if stop {
        shared.stop.notify_one();
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
            Some(error(Value::Null, -32700, "invalid", "not JSON")),
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
        "hello" => (
            Some(error(
                id,
                -32000,
                "invalid",
                "this connection already said hello",
            )),
            false,
        ),
        "host.stop" => {
            let params = params(&message);
            if params.get("stop_terminals").and_then(Value::as_bool) == Some(false) {
                let formats: Vec<String> = params
                    .get("formats")
                    .and_then(Value::as_array)
                    .map(|f| {
                        f.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_else(|| vec!["vt".to_string()]);
                shared.hand_over(&formats).await;
            } else {
                shared.stop_all().await;
            }
            (Some(result(id, json!({}))), true)
        }
        "terminal.signal" => (
            Some(reply(id, send_signal(params(&message), shared))),
            false,
        ),
        "terminal.stop" => (Some(reply(id, stop(params(&message), shared))), false),
        "terminal.spawn" => (
            Some(reply(
                id,
                tokio::task::block_in_place(|| spawn(params(&message), shared)),
            )),
            false,
        ),
        "env.probe" => {
            let refresh = params(&message)
                .get("refresh")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let outcome = tokio::task::block_in_place(|| shared.probe.get(refresh));
            let mut answer = json!({
                "ok": outcome.failure.is_none(),
                "path_entries": outcome.path_entries(),
                "elapsed_ms": outcome.elapsed.as_millis() as u64,
            });
            if let Some(reason) = outcome.failure {
                answer["reason"] = json!(reason);
            }
            (Some(result(id, answer)), false)
        }
        "programs.find" => {
            let names: Vec<String> = params(&message)
                .get("names")
                .and_then(Value::as_array)
                .map(|names| {
                    names
                        .iter()
                        .filter_map(|n| n.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            // the PATH a spawn's request gives, which wins over the probed one
            let path = params(&message)
                .get("path")
                .and_then(Value::as_str)
                .map(str::to_string);
            let found = tokio::task::block_in_place(|| {
                let mut env = shared.probe.get(false).vars;
                if let Some(path) = path {
                    env.retain(|(k, _)| k != "PATH");
                    env.push(("PATH".to_string(), path));
                }
                crate::programs::find(&names, &env)
            });
            (Some(result(id, json!({"programs": found}))), false)
        }
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
        "terminal.submit" => (
            Some(reply(id, submit(params(&message), shared).await)),
            false,
        ),
        "terminal.text" => (Some(reply(id, text(params(&message), shared).await)), false),
        "terminal.screen" => (
            Some(reply(id, screen(params(&message), shared).await)),
            false,
        ),
        _ => (
            Some(error(
                id,
                -32601,
                "unsupported",
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
    /// Stops every live terminal as the host shuts down, and waits until
    /// they have ended.
    /// Hands every live terminal over to the next host, which finds the
    /// screens in `<data dir>/host/handover/`. The programs keep running
    /// in their holders.
    async fn hand_over(&self, formats: &[String]) {
        let answers: Vec<_> = {
            let terminals = self.terminals.lock().unwrap_or_else(|e| e.into_inner());
            terminals
                .iter()
                .filter(|(_, t)| t.info().ended_at.is_none())
                .map(|(id, t)| {
                    t.hand_over(
                        self.hand_over_dir.join(format!("{id}.handover")),
                        formats.to_vec(),
                    )
                })
                .collect()
        };
        for answer in answers {
            let _ = tokio::time::timeout(Duration::from_secs(5), answer).await;
        }
    }

    async fn stop_all(&self) {
        {
            let terminals = self.terminals.lock().unwrap_or_else(|e| e.into_inner());
            for terminal in terminals.values() {
                let _ = terminal.stop(Duration::from_millis(2_000), Reason::HostShutdown);
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.live_terminals() > 0 && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Forgets the terminals earlier hosts ended, by the rule `prune`
    /// applies to this host's own, and the final screens no record names.
    /// The host exits when idle, so these would otherwise stay for good.
    fn forget_earlier(&self) {
        let held: BTreeSet<String> = self
            .terminals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect();
        let records = &self.context.records;
        for id in records.forget_ended(|id| held.contains(id), &prune_cutoff(), KEPT_ENDED) {
            let _ = fs::remove_file(self.context.screens.join(format!("{id}.snap")));
        }
        let Ok(screens) = fs::read_dir(&self.context.screens) else {
            return;
        };
        for entry in screens.flatten() {
            let name = entry.file_name();
            let Some(id) = name.to_str().and_then(|n| n.strip_suffix(".snap")) else {
                continue;
            };
            if !records.has(id) && !held.contains(id) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    /// Forgets ended terminals beyond the newest 200 or older than 24
    /// hours, with their final screens.
    fn prune(&self) {
        let mut terminals = self.terminals.lock().unwrap_or_else(|e| e.into_inner());
        let mut ended: Vec<(String, String)> = terminals
            .values()
            .filter_map(|t| {
                let info = t.info();
                Some((info.ended_at?, info.id))
            })
            .collect();
        ended.sort();
        let cutoff = prune_cutoff();
        let excess = ended.len().saturating_sub(KEPT_ENDED);
        for (n, (ended_at, id)) in ended.into_iter().enumerate() {
            if n < excess || ended_at < cutoff {
                terminals.remove(&id);
                self.context.records.remove(&id);
                let _ = fs::remove_file(self.context.screens.join(format!("{id}.snap")));
            }
        }
    }

    fn live_terminals(&self) -> usize {
        self.terminals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|t| t.live())
            .count()
    }
}

/// When an ended terminal is forgotten: 24 hours after it ended.
fn prune_cutoff() -> String {
    (chrono::Utc::now() - chrono::Duration::hours(24))
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

/// `terminal.spawn` (section 4.3 of the runtime specification).
fn spawn(params: &Value, shared: &Shared) -> Result<Value, Failure> {
    let invalid = |what: &str| Failure("invalid", format!("{what} is missing or invalid"));
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
    // the person's login environment, unless the core asks for the host's
    let base: Vec<(String, String)> = match params.get("env_base").and_then(Value::as_str) {
        Some("host") => std::env::vars().collect(),
        _ => shared.probe.get(false).vars,
    };
    let env = environment::build(&environment::Request {
        base: &base,
        env: &strings("env"),
        session_env: &strings("session_env"),
        cwd,
        terminal: &id,
    });
    let mut record = json!({
        "id": id,
        "holder": null,
        "pid": null,
        "process_started": null,
        "argv0": argv[0],
        "cwd": cwd,
        "labels": labels,
        "cols": cols,
        "rows": rows,
        "started_at": crate::now(),
        "ended_at": null,
        "exit": null,
    });
    // written before the program starts, so a host that crashes during a
    // spawn leaves a record of it
    shared
        .context
        .records
        .insert_now(&id, record.clone())
        .map_err(|e| Failure("spawn_failed", e.to_string()))?;
    if !Path::new(cwd).is_dir() {
        shared.context.records.remove(&id);
        return Err(Failure("folder_missing", format!("{cwd} does not exist")));
    }
    // the recent output, kept for a host that starts after a crash
    let ring = crate::ring::SharedRing::create().ok();
    let (holder, spawned) = shared
        .holders
        .spawn(
            &id,
            &argv,
            cwd,
            &env,
            (cols, rows),
            ring.as_ref().map(|r| r.fd()),
        )
        .map_err(|e| {
            shared.context.records.remove(&id);
            match e {
                SpawnError::FolderMissing => {
                    Failure("folder_missing", format!("{cwd} does not exist"))
                }
                SpawnError::NotFound(name) => Failure("not_found", format!("{name} was not found")),
                SpawnError::Io(error) => Failure("spawn_failed", error.to_string()),
            }
        })?;
    record["holder"] = json!(holder);
    record["pid"] = json!(spawned.pid);
    record["process_started"] = json!(spawned.started);
    shared.context.records.update(&id, |r| *r = record);
    let screen = terminal::screen(cols, rows, SCROLLBACK_LINES)
        .map_err(|e| Failure("spawn_failed", e.to_string()))?;
    let pid = spawned.pid;
    let forget = {
        let (holders, holder, id) = (shared.holders.clone(), holder.clone(), id.clone());
        Box::new(move || holders.forget(&holder, &id))
    };
    let terminal = Terminal::start(
        id.clone(),
        spawned,
        labels.clone(),
        (cols, rows),
        screen,
        shared.context.clone(),
        terminal::Holding {
            forget,
            ring,
            held: Vec::new(),
        },
    )
    .map_err(|e| Failure("spawn_failed", e.to_string()))?;
    let info = terminal.info();
    shared
        .terminals
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id.clone(), terminal);
    shared.context.events.emit(
        "spawned",
        json!({"terminal": id, "pid": pid, "labels": labels, "cols": cols, "rows": rows}),
    );
    Ok(
        json!({"terminal": id, "pid": pid, "process_started": info.process_started,
               "started_at": info.started_at}),
    )
}

/// The terminal a call names, or `not_found`.
fn with_terminal<T>(
    params: &Value,
    shared: &Shared,
    act: impl FnOnce(&Terminal) -> Result<T, Failure>,
) -> Result<T, Failure> {
    let id = params.get("terminal").and_then(Value::as_str).unwrap_or("");
    let terminals = shared.terminals.lock().unwrap_or_else(|e| e.into_inner());
    let terminal = terminals
        .get(id)
        .ok_or_else(|| Failure("not_found", format!("no terminal {id:?}")))?;
    act(terminal)
}

fn signal_failure(error: SignalError) -> Failure {
    match error {
        SignalError::Exited => Failure("exited", "the program has exited".to_string()),
        SignalError::Io(error) => Failure("signal_failed", error.to_string()),
    }
}

/// `terminal.signal {terminal, signal}`.
fn send_signal(params: &Value, shared: &Shared) -> Result<Value, Failure> {
    let name = params.get("signal").and_then(Value::as_str).unwrap_or("");
    let number = terminal::signal_number(name)
        .ok_or_else(|| Failure("invalid", format!("no signal {name:?}")))?;
    with_terminal(params, shared, |t| t.signal(number).map_err(signal_failure))?;
    Ok(json!({}))
}

/// `terminal.stop {terminal, grace_ms}`.
fn stop(params: &Value, shared: &Shared) -> Result<Value, Failure> {
    let grace = params
        .get("grace_ms")
        .and_then(Value::as_u64)
        .unwrap_or(2_000);
    with_terminal(params, shared, |t| {
        t.stop(Duration::from_millis(grace), Reason::Stopped)
            .map_err(signal_failure)
    })?;
    Ok(json!({}))
}

/// `terminal.submit`: answers once the message has been typed, which can
/// take seconds; every call runs in a task of its own, so other calls are
/// not held up.
async fn submit(params: &Value, shared: &Shared) -> Result<Value, Failure> {
    let millis = |key, default| {
        Duration::from_millis(params.get(key).and_then(Value::as_u64).unwrap_or(default))
    };
    let text = params
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| Failure("invalid", "text is missing".to_string()))?;
    let request = terminal::Submit {
        text: text.to_string(),
        bracketed: params
            .get("bracketed")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        settle: millis("settle_ms", 300),
        max: millis("max_ms", 2_000),
        startup_quiet: millis("startup_quiet_ms", 800),
        submit: params
            .get("submit")
            .and_then(Value::as_str)
            .unwrap_or("\r")
            .to_string(),
        expect_pid: params
            .get("expect_pid")
            .and_then(Value::as_u64)
            .and_then(|p| u32::try_from(p).ok()),
        idle_since: params
            .get("idle_since")
            .and_then(Value::as_str)
            .map(str::to_string),
        anyway: params
            .get("anyway")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };
    let typed = with_terminal(params, shared, |t| Ok(t.submit(request)))?;
    typed
        .await
        .map_err(|refused| Failure(refused.code(), format!("{refused:?}")))?;
    Ok(json!({"ok": true}))
}

/// `terminal.attach {terminal, mode, cols, rows, history, formats,
/// resume_from?}`: opens a terminal channel, paused until the reply that
/// names it has been queued, so the core always knows a channel before its
/// first frame. Returns whether the message was one.
fn attach(
    payload: &[u8],
    connection: u64,
    out: &Outbox,
    shared: &Arc<Shared>,
    channels: &Arc<Mutex<BTreeMap<u32, String>>>,
    next_channel: &Arc<std::sync::atomic::AtomicU32>,
) -> bool {
    let Ok(message) = serde_json::from_slice::<Value>(payload) else {
        return false;
    };
    if message.get("method").and_then(Value::as_str) != Some("terminal.attach") {
        return false;
    }
    let Some(id) = message.get("id").cloned() else {
        return true;
    };
    let params = params(&message).clone();
    let size = |key| {
        params
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|n| u16::try_from(n).ok())
            .filter(|&n| n > 0)
    };
    let (Some(cols), Some(rows)) = (size("cols"), size("rows")) else {
        out.reply(&error(id, -32000, "invalid", "cols and rows are required"));
        return true;
    };
    let request = AttachRequest {
        cols,
        rows,
        history: params
            .get("history")
            .and_then(Value::as_u64)
            .unwrap_or(5_000) as usize,
        formats: params
            .get("formats")
            .and_then(Value::as_array)
            .map(|f| {
                f.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        resume_from: params.get("resume_from").and_then(Value::as_u64),
    };
    let terminal_id = params
        .get("terminal")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let channel = next_channel.fetch_add(1, Ordering::Relaxed);
    let viewer = (connection, channel);
    let attached = {
        let terminals = shared.terminals.lock().unwrap_or_else(|e| e.into_inner());
        terminals
            .get(&terminal_id)
            .map(|t| t.attach(viewer, out.clone(), request))
    };
    let (out, shared, channels) = (out.clone(), shared.clone(), channels.clone());
    tokio::spawn(async move {
        let Some(attached) = attached else {
            out.reply(&error(
                id,
                -32000,
                "not_found",
                &format!("no terminal {terminal_id:?}"),
            ));
            return;
        };
        match attached.await {
            Ok(format) => {
                channels
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(channel, terminal_id.clone());
                out.reply(&result(id, json!({"channel": channel, "format": format})));
                if let Some(t) = shared
                    .terminals
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&terminal_id)
                {
                    t.resume(viewer);
                }
            }
            Err(AttachError::Exited) => {
                out.reply(&error(id, -32000, "exited", "the program has exited"));
            }
            Err(AttachError::Unsupported) => {
                out.reply(&error(
                    id,
                    -32000,
                    "unsupported",
                    "none of these snapshot formats",
                ));
            }
        }
    });
    true
}

/// `events.subscribe {after?, instance?}`, answered here so that the reply
/// and the events after it go out in order. Returns whether the message
/// was one.
fn subscribe(payload: &[u8], connection: u64, out: &Outbox, shared: &Shared) -> bool {
    let Ok(message) = serde_json::from_slice::<Value>(payload) else {
        return false;
    };
    if message.get("method").and_then(Value::as_str) != Some("events.subscribe") {
        return false;
    }
    let Some(id) = message.get("id").cloned() else {
        return true;
    };
    let params = params(&message);
    // an `after` from another instance, or with none, is not a position here
    let after = params
        .get("after")
        .and_then(Value::as_u64)
        .filter(|_| params.get("instance").and_then(Value::as_str) == Some(&shared.instance));
    let terminals: Vec<Value> = shared
        .terminals
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .map(|t| t.info().to_json())
        .collect();
    shared
        .context
        .events
        .subscribe(connection, out, after, |seq| {
            result(
                id,
                json!({"instance": shared.instance, "seq": seq, "terminals": terminals}),
            )
        });
    true
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

/// `terminal.screen`: the screen as a paint any emulator reads, with its
/// size, which for an ended terminal is the one it ended with.
async fn screen(params: &Value, shared: &Shared) -> Result<Value, Failure> {
    let id = params.get("terminal").and_then(Value::as_str).unwrap_or("");
    let request = {
        let terminals = shared.terminals.lock().unwrap_or_else(|e| e.into_inner());
        let terminal = terminals
            .get(id)
            .ok_or_else(|| Failure("not_found", format!("no terminal {id:?}")))?;
        terminal.paint_request()
    };
    let (paint, (cols, rows)) = request
        .await
        .ok_or_else(|| Failure("not_found", format!("no terminal {id:?}")))?;
    Ok(json!({"paint": String::from_utf8_lossy(&paint), "cols": cols, "rows": rows}))
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
