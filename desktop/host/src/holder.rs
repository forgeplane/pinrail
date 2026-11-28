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
//!
//! On Windows, where a pseudo console cannot be handed to another process,
//! the holder keeps it and gives the host its input and output pipes and
//! its job; two more requests, `resize` and `close`, act on the console
//! for the host.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::fdpass::{self, Owned};
use crate::net::{Listener, Stream};
use crate::paths::Paths;
use crate::pty::{self, Exit, Holding, SpawnError};
use crate::registration::Registration;

/// The holder's protocol. It is frozen: a release that needs another
/// starts a new holder for new terminals, and keeps talking to this one.
pub const PROTOCOL: u32 = 1;
/// How long a holder with live terminals waits for a host before it starts
/// one, and how long it waits before the first retry.
pub const HOST_WITHIN: Duration = Duration::from_secs(5);
/// The longest wait between two starts of a host.
pub const HOST_RETRY_MAX: Duration = Duration::from_secs(60);

/// How long a holder with no terminal and no host waits for one to
/// connect before it exits.
pub const ALONE_EXIT: Duration = Duration::from_secs(30);

/// A terminal the holder keeps.
struct Held {
    holding: Holding,
    ring: Option<Owned>,
    exit: Option<Exit>,
}

/// The holder's end of its connection to a host, for what it sends.
struct Host {
    stream: Stream,
    /// Held for each message sent: answers and events go from several
    /// threads, and a message written in parts must not have another's
    /// between them.
    sending: Mutex<()>,
}

impl Host {
    /// Sends one message whole. It waits for as long as the host does not
    /// read, so it is never called with the holder's state locked: a host
    /// that hangs then holds up what is sent to it and nothing else. The
    /// wait ends when the connection is shut down, as it is when another
    /// host connects.
    fn send(&self, value: &Value, fds: &[fdpass::Raw]) {
        let _whole = self.sending.lock().unwrap_or_else(|e| e.into_inner());
        let _ = fdpass::send(&self.stream, value, fds);
    }
}

#[derive(Default)]
struct State {
    terminals: BTreeMap<String, Held>,
    /// The connected host, for replies and events, and its connection's
    /// number.
    host: Option<Arc<Host>>,
    connection: u64,
    /// When the last host left, or the holder started.
    alone_since: Option<Instant>,
}

type Shared = Arc<Mutex<State>>;

fn lock(state: &Shared) -> std::sync::MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

/// A holder that listens, ready to run.
pub struct Listening {
    instance: String,
    socket: PathBuf,
    listener: Listener,
    registration: PathBuf,
    _lock: File,
}

/// Starts a holder for `data_dir`: takes its lock and listens on its
/// socket. The agents inherit its open-file limit, which it raises first,
/// since launchd starts processes with 256.
pub fn listen(data_dir: &Path) -> io::Result<Listening> {
    let paths = Paths::new(data_dir);
    paths.create()?;
    crate::sys::raise_open_files(8_192);
    let instance = crate::random_hex(4);
    let holders = paths.host_dir.join("holders");
    std::fs::create_dir_all(&holders)?;
    let lock = take_lock(&holders.join(format!("{instance}.lock")))?;
    let socket = paths.holder_socket(&instance);
    let listener = crate::net::listen(&socket)?;
    listener.set_nonblocking(true)?;
    Ok(Listening {
        instance,
        socket,
        listener,
        registration: paths.registration(),
        _lock: lock,
    })
}

impl Listening {
    pub fn instance(&self) -> &str {
        &self.instance
    }

    /// Serves hosts until the holder has kept no terminal and had no host
    /// connected for `alone_exit`.
    ///
    /// With `host`, the binary and data folder to start a host from: when
    /// the holder keeps live terminals and no host has connected for
    /// [`HOST_WITHIN`], it starts one, and tries again after a delay that
    /// doubles up to [`HOST_RETRY_MAX`]. Without a host, an agent that
    /// writes while the app is closed would block on its full PTY. The
    /// host's lock keeps a second host from running.
    pub fn run(self, alone_exit: Duration, host: Option<(PathBuf, PathBuf)>) -> io::Result<()> {
        let state: Shared = Arc::new(Mutex::new(State {
            alone_since: Some(Instant::now()),
            ..State::default()
        }));
        // when to start a host next, and how many starts were made
        let mut restart: Option<(Instant, u32)> = None;
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    let state = state.clone();
                    let (registration, instance) =
                        (self.registration.clone(), self.instance.clone());
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
                crate::net::remove(&self.socket);
                return Ok(());
            }
            let (live, connected) = {
                let state = lock(&state);
                let live = state.terminals.values().any(|held| held.exit.is_none());
                (live, state.host.is_some())
            };
            let Some((binary, data_dir)) = &host else {
                continue;
            };
            if !live || connected {
                restart = None;
                continue;
            }
            let now = Instant::now();
            let (due, starts) = *restart.get_or_insert((now + HOST_WITHIN, 0));
            if now >= due {
                start_host(binary, data_dir);
                restart = Some((now + retry_after(starts + 1), starts + 1));
            }
        }
    }
}

/// The wait after the `starts`-th start of a host: 5 seconds, 5 again,
/// then doubling up to a minute.
fn retry_after(starts: u32) -> Duration {
    let doubled = HOST_WITHIN.saturating_mul(1 << starts.saturating_sub(1).min(8));
    doubled.min(HOST_RETRY_MAX)
}

/// Starts `<binary> serve` for `data_dir`, detached from this holder, and
/// does not wait for it.
#[cfg(windows)]
fn start_host(binary: &Path, data_dir: &Path) {
    let mut command = std::process::Command::new(binary);
    command
        .arg("serve")
        .arg("--data-dir")
        .arg(data_dir)
        .stdout(std::process::Stdio::null());
    if let Ok(mut child) = crate::sys::spawn_detached(command) {
        std::thread::spawn(move || child.wait());
    }
}

/// Starts `<binary> serve` for `data_dir` in a session of its own, with
/// nothing on its standard streams, and does not wait for it.
#[cfg(unix)]
#[allow(unsafe_code)]
fn start_host(binary: &Path, data_dir: &Path) {
    use std::os::unix::process::CommandExt;
    let mut command = std::process::Command::new(binary);
    command
        .arg("serve")
        .arg("--data-dir")
        .arg(data_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // SAFETY: setsid is async-signal-safe and allocates nothing.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    if let Ok(mut child) = command.spawn() {
        // reaped here; the host stays in its own session
        std::thread::spawn(move || child.wait());
    }
}

/// Runs a holder for `data_dir` until it has kept no terminal and had no
/// host connected for `alone_exit`. Prints its instance id on standard
/// output once it listens.
pub fn hold(data_dir: &Path, alone_exit: Duration) -> io::Result<()> {
    let listening = listen(data_dir)?;
    println!("{}", listening.instance());
    // the host reads the line, then no more
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let host = std::env::current_exe()
        .ok()
        .map(|binary| (binary, data_dir.to_path_buf()));
    listening.run(alone_exit, host)
}

fn take_lock(path: &Path) -> io::Result<File> {
    let mut file = OpenOptions::new();
    file.read(true).write(true).create(true).truncate(false);
    let file = crate::private(&mut file).open(path)?;
    file.try_lock().map_err(io::Error::other)?;
    // the holder's process, for status and for tests to clean up
    use std::io::Write;
    file.set_len(0)?;
    (&file).write_all(std::process::id().to_string().as_bytes())?;
    Ok(file)
}

/// One host's connection: the handshake, then its requests until it
/// leaves. A host that connects replaces the one before it.
fn serve(stream: Stream, state: &Shared, registration: &Path, instance: &str) {
    let _ = stream.set_nonblocking(false);
    if !handshake(&stream, registration, instance) {
        return;
    }
    let Ok(writer) = stream.try_clone() else {
        return;
    };
    let host = Arc::new(Host {
        stream: writer,
        sending: Mutex::new(()),
    });
    let (mine, exits) = {
        let mut state = lock(state);
        if let Some(old) = state.host.replace(host.clone()) {
            // which also ends a write that waits for the old host to read
            let _ = old.stream.shutdown(std::net::Shutdown::Both);
        }
        state.connection += 1;
        state.alone_since = None;
        // exits that came while no host was connected. An exit after this
        // finds the host in the state, and is sent by the one that saw it.
        let exits: Vec<(String, Exit)> = state
            .terminals
            .iter()
            .filter_map(|(id, held)| Some((id.clone(), held.exit?)))
            .collect();
        (state.connection, exits)
    };
    for (terminal, exit) in exits {
        host.send(&exited(&terminal, exit), &[]);
    }
    while let Ok(Some((request, fds))) = fdpass::recv(&stream) {
        let id = request["id"].clone();
        let mut fds = fds.into_iter();
        let (mut answer, out) = match request["op"].as_str() {
            Some("spawn") => spawn(state, &request, fds.next()),
            Some("list") => list(state, &request),
            Some("forget") => forget(state, &request),
            #[cfg(windows)]
            Some("resize") => resize(state, &request),
            #[cfg(windows)]
            Some("close") => close(state, &request),
            _ => (
                json!({"error": "invalid", "message": "unknown op"}),
                Vec::new(),
            ),
        };
        answer["id"] = id;
        // a host that replaced this one is asked nothing more by it; the
        // answer goes on this connection, never on the new host's, whose
        // own requests have the same ids
        if lock(state).connection != mine {
            break;
        }
        let raw: Vec<fdpass::Raw> = out.iter().map(fdpass::raw).collect();
        host.send(&answer, &raw);
    }
    let mut state = lock(state);
    if state.connection == mine {
        state.host = None;
        state.alone_since = Some(Instant::now());
    }
}

/// The host proves it is the host: a process of this user that knows the
/// host secret.
fn handshake(stream: &Stream, registration: &Path, instance: &str) -> bool {
    // who it is first, and nothing it sends is taken before it is known
    let same_user = crate::net::same_user(stream);
    let Ok(Some(hello)) = fdpass::recv_plain(stream) else {
        return false;
    };
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
/// descriptor, or on Windows the console's input and output and the job.
fn spawn(state: &Shared, request: &Value, ring: Option<Owned>) -> (Value, Vec<Owned>) {
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
    let spawned = match Holding::spawn(&argv, &cwd, &env, size("cols"), size("rows")) {
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
    let (for_host, waiter) = match (spawned.for_host(), spawned.exit_waiter()) {
        (Ok(for_host), Ok(waiter)) => (for_host, waiter),
        (Err(error), _) | (_, Err(error)) => {
            spawned.forget(false);
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
            holding: spawned,
            ring,
            exit: None,
        },
    );
    let state = state.clone();
    std::thread::spawn(move || watch_exit(&state, &terminal, pid, waiter));
    (answer, for_host)
}

/// Waits for the program to exit, leaves it unreaped, and tells the host.
fn watch_exit(state: &Shared, terminal: &str, pid: u32, waiter: impl FnOnce() -> io::Result<Exit>) {
    let Ok(exit) = waiter() else {
        return;
    };
    let host = {
        let mut state = lock(state);
        match state.terminals.get_mut(terminal) {
            Some(held) if held.holding.pid == pid => {
                held.exit = Some(exit);
                held.holding.exited();
                state.host.clone()
            }
            // forgotten before it exited: nothing will signal it now
            _ => {
                pty::reap(pid);
                return;
            }
        }
    };
    // a host that connects from here on is told of the exit as it connects
    if let Some(host) = host {
        host.send(&exited(terminal, exit), &[]);
    }
}

/// `list`: every terminal, with duplicates of what the host gets of it
/// and of its ring descriptor, in that order, and its size.
///
/// A host asks for them a page at a time, `{"after", "limit"}`, as one
/// message carries the descriptors of some 80 terminals at most: the
/// answer says whether `more` follow. Asked without a limit, as a host of
/// an earlier build asks, it lists them all.
fn list(state: &Shared, request: &Value) -> (Value, Vec<Owned>) {
    let state = lock(state);
    let after = request["after"].as_str();
    let limit = request["limit"].as_u64().map(|n| n as usize);
    let mut terminals = Vec::new();
    let mut fds = Vec::new();
    let mut more = false;
    let page = state
        .terminals
        .iter()
        .filter(|(id, _)| after.is_none_or(|after| id.as_str() > after));
    for (id, held) in page {
        if limit.is_some_and(|limit| terminals.len() >= limit) {
            more = true;
            break;
        }
        let (Ok(master), ring) = (
            held.holding.for_host(),
            held.ring.as_ref().and_then(|r| r.try_clone().ok()),
        ) else {
            continue;
        };
        let (cols, rows) = held.holding.size();
        let mut entry = json!({"terminal": id, "pid": held.holding.pid,
                               "started": held.holding.started,
                               "ring": ring.is_some(), "cols": cols, "rows": rows});
        if let Some(exit) = held.exit {
            entry["exit"] = match exit {
                Exit::Code(code) => json!({"code": code}),
                Exit::Signal(signal) => json!({"signal": signal}),
            };
        }
        terminals.push(entry);
        fds.extend(master);
        fds.extend(ring);
    }
    (json!({"terminals": terminals, "more": more}), fds)
}

/// `forget {terminal}`: closes the terminal's descriptors, and reaps the
/// program if it has exited.
fn forget(state: &Shared, request: &Value) -> (Value, Vec<Owned>) {
    let terminal = request["terminal"].as_str().unwrap_or_default();
    let mut state = lock(state);
    if let Some(held) = state.terminals.remove(terminal) {
        held.holding.forget(held.exit.is_some());
    }
    (json!({"ok": true}), Vec::new())
}

/// `resize {terminal, cols, rows}`: resizes the pseudo console.
#[cfg(windows)]
fn resize(state: &Shared, request: &Value) -> (Value, Vec<Owned>) {
    let size = |key: &str| {
        request[key]
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .filter(|&n| n > 0)
    };
    let terminal = request["terminal"].as_str().unwrap_or_default();
    let state = lock(state);
    let answer = match (state.terminals.get(terminal), size("cols"), size("rows")) {
        (Some(held), Some(cols), Some(rows)) => match held.holding.resize(cols, rows) {
            Ok(()) => json!({"ok": true}),
            Err(error) => json!({"error": "resize_failed", "message": error.to_string()}),
        },
        (None, ..) => json!({"error": "not_found", "message": "no such terminal"}),
        _ => json!({"error": "invalid", "message": "no size"}),
    };
    (answer, Vec::new())
}

/// `close {terminal}`: closes the pseudo console, which tells its programs
/// that their console is gone, as a hangup does on Unix.
#[cfg(windows)]
fn close(state: &Shared, request: &Value) -> (Value, Vec<Owned>) {
    let terminal = request["terminal"].as_str().unwrap_or_default();
    let state = lock(state);
    let answer = match state.terminals.get(terminal) {
        Some(held) => {
            held.holding.close();
            json!({"ok": true})
        }
        None => json!({"error": "not_found", "message": "no such terminal"}),
    };
    (answer, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A handshake from someone who is not the host, with a handle of this
    /// process in it: the holder must not take it, and so close it.
    #[cfg(windows)]
    #[test]
    #[allow(unsafe_code)]
    fn a_handshake_takes_no_handle_from_whoever_connects() {
        use std::io::Write;
        use std::os::windows::io::AsRawHandle;
        let path = std::path::PathBuf::from(format!(
            r"\\.\pipe\pinrail-handshake-test-{}",
            crate::random_hex(4)
        ));
        let listener = crate::net::listen(&path).unwrap();
        let client = crate::net::connect(&path).unwrap();
        let (server, _) = listener.accept().unwrap();
        let mine = crate::win::event().unwrap();
        let mut hello = 2u32.to_le_bytes().to_vec();
        hello.extend(b"{}");
        hello.extend(1u32.to_le_bytes());
        hello.extend((mine.as_raw_handle() as u64).to_le_bytes());
        (&client).write_all(&hello).unwrap();
        let registration = tempfile::tempdir().unwrap().path().join("none.json");
        assert!(!handshake(&server, &registration, "i"));
        let mut flags = 0;
        // SAFETY: asks about a handle this test still owns.
        let open = unsafe {
            windows_sys::Win32::Foundation::GetHandleInformation(crate::win::raw(&mine), &mut flags)
        };
        assert_ne!(open, 0, "the handshake closed a handle of this process");
    }

    #[test]
    fn the_wait_between_starts_of_a_host_doubles_up_to_a_minute() {
        let waits: Vec<u64> = (1..=7).map(|n| retry_after(n).as_secs()).collect();
        assert_eq!(waits, [5, 10, 20, 40, 60, 60, 60]);
    }
}
