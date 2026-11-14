//! `pinrail-host hold`: the holder, the parent of every agent the host
//! starts. It opens each PTY and spawns the program when the host asks,
//! keeps the PTY's master descriptor and the terminal's output-ring
//! descriptor open, and gives the host duplicates of both. It never reads
//! or parses the PTY or the ring, so it has no reason to crash, and a host
//! that crashes or is replaced ends no agent: the next host takes the
//! descriptors back.
//!
//! The protocol, after a handshake with the host secret, has four
//! messages, and is frozen: `spawn`, `list`, `forget`, and the `exited`
//! event. The holder observes an exit without reaping the program, and
//! reaps it only on `forget`, so that the host's signals cannot reach
//! another process given the same id.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::fdpass;
use crate::paths::Paths;
use crate::pty::{self, Exit, SpawnError};
use crate::registration::Registration;

/// The holder's protocol. It is frozen: a release that needs another
/// starts a new holder for new terminals, and keeps talking to this one.
pub const PROTOCOL: u32 = 1;
/// How long a holder with no terminal and no host waits for one to
/// connect before it exits.
pub const ALONE_EXIT: Duration = Duration::from_secs(30);

/// A terminal the holder keeps.
struct Held {
    pid: u32,
    started: Option<String>,
    master: OwnedFd,
    ring: Option<OwnedFd>,
    exit: Option<Exit>,
}

#[derive(Default)]
struct State {
    terminals: BTreeMap<String, Held>,
    /// The connected host, for replies and events, and its connection's
    /// number.
    host: Option<UnixStream>,
    connection: u64,
    /// When the last host left, or the holder started.
    alone_since: Option<Instant>,
}

type Shared = Arc<Mutex<State>>;

fn lock(state: &Shared) -> std::sync::MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

/// A holder's socket in `socket_dir`, for its instance id.
pub fn socket_path(socket_dir: &Path, instance: &str) -> PathBuf {
    socket_dir.join(format!("holder.{instance}.sock"))
}

/// The instances of the holders whose sockets are in `socket_dir`.
pub fn instances(socket_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(socket_dir) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_prefix("holder.")?
                .strip_suffix(".sock")
                .map(str::to_string)
        })
        .collect();
    found.sort();
    found
}

/// Runs a holder for `data_dir` until it has kept no terminal and had no
/// host connected for `alone_exit`. Prints its instance id on standard
/// output once it listens.
pub fn hold(data_dir: &Path, alone_exit: Duration) -> io::Result<()> {
    let paths = Paths::new(data_dir);
    paths.create()?;
    // the agents inherit the limit, and launchd starts processes with 256
    crate::sys::raise_open_files(8_192);
    let instance = crate::random_hex(4);
    let holders = paths.host_dir.join("holders");
    std::fs::create_dir_all(&holders)?;
    let _lock = take_lock(&holders.join(format!("{instance}.lock")))?;
    let socket = socket_path(&paths.socket_dir, &instance);
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    println!("{instance}");
    let state: Shared = Arc::new(Mutex::new(State {
        alone_since: Some(Instant::now()),
        ..State::default()
    }));
    let registration = paths.registration();
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let state = state.clone();
                let (registration, instance) = (registration.clone(), instance.clone());
                std::thread::spawn(move || serve(stream, &state, &registration, &instance));
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e),
        }
        let done = {
            let state = lock(&state);
            state.terminals.is_empty()
                && state.host.is_none()
                && state
                    .alone_since
                    .is_some_and(|since| since.elapsed() >= alone_exit)
        };
        if done {
            let _ = std::fs::remove_file(&socket);
            return Ok(());
        }
    }
}

fn take_lock(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    file.try_lock().map_err(io::Error::other)?;
    Ok(file)
}

/// One host's connection: the handshake, then its requests until it
/// leaves. A host that connects replaces the one before it.
fn serve(stream: UnixStream, state: &Shared, registration: &Path, instance: &str) {
    let _ = stream.set_nonblocking(false);
    if !handshake(&stream, registration, instance) {
        return;
    }
    let Ok(writer) = stream.try_clone() else {
        return;
    };
    let mine = {
        let mut state = lock(state);
        if let Some(old) = state.host.replace(writer) {
            let _ = old.shutdown(std::net::Shutdown::Both);
        }
        state.connection += 1;
        state.alone_since = None;
        // exits that came while no host was connected
        let exits: Vec<(String, Exit)> = state
            .terminals
            .iter()
            .filter_map(|(id, held)| Some((id.clone(), held.exit?)))
            .collect();
        for (terminal, exit) in exits {
            send_on(&state, &exited(&terminal, exit), &[]);
        }
        state.connection
    };
    while let Ok(Some((request, fds))) = fdpass::recv(&stream) {
        let id = request["id"].clone();
        let mut fds = fds.into_iter();
        let (mut answer, out) = match request["op"].as_str() {
            Some("spawn") => spawn(state, &request, fds.next()),
            Some("list") => list(state),
            Some("forget") => forget(state, &request),
            _ => (
                json!({"error": "invalid", "message": "unknown op"}),
                Vec::new(),
            ),
        };
        answer["id"] = id;
        let state = lock(state);
        let raw: Vec<i32> = out.iter().map(AsRawFd::as_raw_fd).collect();
        send_on(&state, &answer, &raw);
    }
    let mut state = lock(state);
    if state.connection == mine {
        state.host = None;
        state.alone_since = Some(Instant::now());
    }
}

/// The host proves it is the host: a process of this user that knows the
/// host secret.
fn handshake(stream: &UnixStream, registration: &Path, instance: &str) -> bool {
    let Ok(Some((hello, _))) = fdpass::recv(stream) else {
        return false;
    };
    let same_user = fdpass::peer_uid(stream).is_ok_and(|uid| uid == crate::sys::uid());
    let secret = Registration::read(registration).map(|r| r.secret);
    let knows = matches!((&secret, hello["secret"].as_str()), (Some(s), Some(given)) if s == given);
    if !same_user || !knows {
        let _ = fdpass::send(
            stream,
            &json!({"error": "forbidden", "message": "not the host"}),
            &[],
        );
        return false;
    }
    fdpass::send(
        stream,
        &json!({"instance": instance, "protocol": PROTOCOL, "pid": std::process::id()}),
        &[],
    )
    .is_ok()
}

fn send_on(state: &State, value: &Value, fds: &[i32]) {
    if let Some(host) = &state.host {
        let _ = fdpass::send(host, value, fds);
    }
}

fn exited(terminal: &str, exit: Exit) -> Value {
    let mut event = json!({"event": "exited", "terminal": terminal});
    match exit {
        Exit::Code(code) => event["code"] = json!(code),
        Exit::Signal(signal) => event["signal"] = json!(signal),
    }
    event
}

/// `spawn {terminal, argv, cwd, env, cols, rows}` with the ring's
/// descriptor: answers with the pid and start time, and the master's
/// descriptor.
fn spawn(state: &Shared, request: &Value, ring: Option<OwnedFd>) -> (Value, Vec<OwnedFd>) {
    let Some(terminal) = request["terminal"].as_str().map(str::to_string) else {
        return (
            json!({"error": "invalid", "message": "no terminal"}),
            Vec::new(),
        );
    };
    let argv: Vec<String> = request["argv"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if argv.is_empty() {
        return (
            json!({"error": "invalid", "message": "no argv"}),
            Vec::new(),
        );
    }
    let env: BTreeMap<String, String> = request["env"]
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let size = |key: &str| {
        request[key]
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .unwrap_or(80)
    };
    let cwd = PathBuf::from(request["cwd"].as_str().unwrap_or_default());
    let spawned = match pty::spawn(&argv, &cwd, &env, size("cols"), size("rows")) {
        Ok(spawned) => spawned,
        Err(SpawnError::FolderMissing) => {
            return (
                json!({"error": "folder_missing", "message": format!("{} does not exist", cwd.display())}),
                Vec::new(),
            );
        }
        Err(SpawnError::NotFound(name)) => {
            return (
                json!({"error": "not_found", "message": format!("{name} was not found")}),
                Vec::new(),
            );
        }
        Err(SpawnError::Io(error)) => {
            return (
                json!({"error": "spawn_failed", "message": error.to_string()}),
                Vec::new(),
            );
        }
    };
    let master = match spawned.master.try_clone() {
        Ok(fd) => fd,
        Err(error) => {
            return (
                json!({"error": "spawn_failed", "message": error.to_string()}),
                Vec::new(),
            );
        }
    };
    let pid = spawned.pid;
    let answer = json!({"pid": pid, "started": spawned.started});
    lock(state).terminals.insert(
        terminal.clone(),
        Held {
            pid,
            started: spawned.started,
            master: spawned.master,
            ring,
            exit: None,
        },
    );
    let state = state.clone();
    std::thread::spawn(move || watch_exit(&state, &terminal, pid));
    (answer, vec![master])
}

/// Waits for the program to exit, leaves it unreaped, and tells the host.
fn watch_exit(state: &Shared, terminal: &str, pid: u32) {
    let Ok(exit) = pty::wait_exit(pid) else {
        return;
    };
    let mut state = lock(state);
    match state.terminals.get_mut(terminal) {
        Some(held) if held.pid == pid => {
            held.exit = Some(exit);
            send_on(&state, &exited(terminal, exit), &[]);
        }
        // forgotten before it exited: nothing will signal it now
        _ => pty::reap(pid),
    }
}

/// `list`: every terminal, with duplicates of its master and ring
/// descriptors, in that order.
fn list(state: &Shared) -> (Value, Vec<OwnedFd>) {
    let state = lock(state);
    let mut terminals = Vec::new();
    let mut fds = Vec::new();
    for (id, held) in &state.terminals {
        let (Ok(master), ring) = (
            held.master.try_clone(),
            held.ring.as_ref().and_then(|r| r.try_clone().ok()),
        ) else {
            continue;
        };
        let mut entry = json!({"terminal": id, "pid": held.pid, "started": held.started,
                               "ring": ring.is_some()});
        if let Some(exit) = held.exit {
            entry["exit"] = match exit {
                Exit::Code(code) => json!({"code": code}),
                Exit::Signal(signal) => json!({"signal": signal}),
            };
        }
        terminals.push(entry);
        fds.push(master);
        fds.extend(ring);
    }
    (json!({"terminals": terminals}), fds)
}

/// `forget {terminal}`: closes the terminal's descriptors, and reaps the
/// program if it has exited.
fn forget(state: &Shared, request: &Value) -> (Value, Vec<OwnedFd>) {
    let terminal = request["terminal"].as_str().unwrap_or_default();
    let mut state = lock(state);
    if let Some(held) = state.terminals.remove(terminal)
        && held.exit.is_some()
    {
        pty::reap(held.pid);
    }
    (json!({"ok": true}), Vec::new())
}
