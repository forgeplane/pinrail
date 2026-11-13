//! One terminal: a reader thread that reads the PTY, an actor task that
//! owns the screen and applies everything in one order, a writer thread
//! that writes to the PTY, and a waiter thread that sees the program exit.

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pinrail_screen::{Options, Screen, Signal, TextRange};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use crate::events::Events;
use crate::pty::{self, Exit, Ready, Spawned};
use crate::records::Records;

/// The most the reader reads at once.
const CHUNK: usize = 64 * 1024;
/// Chunks waiting for the actor before the reader stops reading, which
/// holds at most 8 MiB, so a runaway program blocks as it would on a PTY
/// nobody reads.
const QUEUED_CHUNKS: usize = 8 * 1024 * 1024 / CHUNK;
/// How long the actor waits for the last output after the program exits.
/// The count starts again with each chunk that arrives, so an exit never
/// overtakes its output.
const EXIT_DRAIN: Duration = Duration::from_millis(200);

/// What the core and `terminal.list` see of a terminal.
#[derive(Debug, Clone, PartialEq)]
pub struct Info {
    pub id: String,
    pub pid: u32,
    pub labels: Value,
    pub cols: u16,
    pub rows: u16,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub exit: Option<Exit>,
}

impl Info {
    pub fn to_json(&self) -> Value {
        let mut value = json!({
            "terminal": self.id,
            "pid": self.pid,
            "labels": self.labels,
            "cols": self.cols,
            "rows": self.rows,
            "started_at": self.started_at,
        });
        if let Some(ended_at) = &self.ended_at {
            value["ended_at"] = json!(ended_at);
        }
        if let Some(exit) = self.exit {
            value["exit"] = exit_json(exit);
        }
        value
    }
}

pub fn exit_json(exit: Exit) -> Value {
    match exit {
        Exit::Code(code) => json!({"code": code}),
        Exit::Signal(signal) => json!({"signal": signal}),
    }
}

/// Why a terminal ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The program exited on its own.
    Exited,
    /// `terminal.stop` stopped it.
    Stopped,
    /// A signal ended it, other than by a stop.
    Signaled,
    /// The host stopped it as it shut down.
    HostShutdown,
}

impl Reason {
    fn as_str(self) -> &'static str {
        match self {
            Reason::Exited => "exited",
            Reason::Stopped => "stopped",
            Reason::Signaled => "signaled",
            Reason::HostShutdown => "host_shutdown",
        }
    }
}

/// The signals the protocol names.
pub fn signal_number(name: &str) -> Option<i32> {
    match name {
        "INT" => Some(libc::SIGINT),
        "HUP" => Some(libc::SIGHUP),
        "TERM" => Some(libc::SIGTERM),
        "KILL" => Some(libc::SIGKILL),
        _ => None,
    }
}

/// How long `terminal.stop` waits after `SIGHUP` before `SIGTERM`.
const AFTER_HANGUP: Duration = Duration::from_millis(500);

enum Output {
    Bytes(Vec<u8>),
    /// The reader stopped: end of file, an error, or a wake.
    Done,
}

enum Command {
    Exited(Exit),
    Text {
        lines: usize,
        reply: oneshot::Sender<String>,
    },
}

/// What the actor and the handle share, under one lock: signals are sent
/// while it is held, and the program is reaped while it is held, so a
/// signal can never reach a process that was given the reaped one's id.
struct State {
    info: Info,
    /// The master, for the foreground group; closed when the terminal ends.
    master: Option<OwnedFd>,
    /// Set by a stop, so the exit is reported with its reason.
    stopping: Option<Reason>,
}

/// What a terminal reports to and keeps for the rest of the host.
#[derive(Clone)]
pub struct Context {
    pub events: Arc<Events>,
    pub records: Arc<Records>,
    /// Where the final screen is kept.
    pub screens: PathBuf,
}

/// A running or ended terminal. Dropping it ends its actor.
pub struct Terminal {
    state: Arc<Mutex<State>>,
    commands: mpsc::UnboundedSender<Command>,
}

/// A failed signal.
#[derive(Debug)]
pub enum SignalError {
    /// The program has exited.
    Exited,
    Io(io::Error),
}

impl Terminal {
    /// Starts the threads and the actor of a spawned program, whose screen
    /// is `screen`. Must be called inside the runtime.
    pub fn start(
        id: String,
        spawned: Spawned,
        labels: Value,
        (cols, rows): (u16, u16),
        screen: Box<dyn Screen>,
        context: Context,
    ) -> io::Result<Self> {
        let pid = spawned.pid;
        let state = Arc::new(Mutex::new(State {
            info: Info {
                id: id.clone(),
                pid,
                labels,
                cols,
                rows,
                started_at: crate::now(),
                ended_at: None,
                exit: None,
            },
            master: Some(spawned.master.try_clone()?),
            stopping: None,
        }));
        let (wake_read, wake_write) = io::pipe()?;
        let (output_tx, output_rx) = mpsc::channel(QUEUED_CHUNKS);
        let (commands, commands_rx) = mpsc::unbounded_channel();
        let (input_tx, input_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let reader_master = File::from(spawned.master.try_clone()?);
        let writer_master = File::from(spawned.master);
        std::thread::Builder::new()
            .name("pty-reader".to_string())
            .spawn(move || read_pty(reader_master, wake_read, output_tx))?;
        std::thread::Builder::new()
            .name("pty-writer".to_string())
            .spawn(move || write_pty(writer_master, input_rx))?;
        let exited = commands.clone();
        std::thread::Builder::new()
            .name("pty-waiter".to_string())
            .spawn(move || {
                let exit = pty::wait_exit(pid).unwrap_or(Exit::Code(-1));
                let _ = exited.send(Command::Exited(exit));
            })?;
        tokio::spawn(actor(Actor {
            id,
            screen,
            state: state.clone(),
            input: Some(input_tx),
            wake: Some(wake_write),
            pid,
            output: output_rx,
            commands: commands_rx,
            context,
            title: None,
        }));
        Ok(Self { state, commands })
    }

    pub fn info(&self) -> Info {
        self.lock().info.clone()
    }

    pub fn live(&self) -> bool {
        self.lock().info.ended_at.is_none()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Sends a signal to the program's process group and, if it differs,
    /// to the PTY's foreground process group.
    pub fn signal(&self, signal: i32) -> Result<(), SignalError> {
        let state = self.lock();
        send(&state, signal)
    }

    /// Stops the program: `SIGHUP`, then `SIGTERM` after 500 ms, then
    /// `SIGKILL` after `grace`, each only if it still runs. Returns at
    /// once; the exit follows as an event.
    pub fn stop(&self, grace: Duration, reason: Reason) -> Result<(), SignalError> {
        {
            let mut state = self.lock();
            if state.info.ended_at.is_some() {
                return Err(SignalError::Exited);
            }
            state.stopping.get_or_insert(reason);
        }
        let state = self.state.clone();
        tokio::spawn(async move {
            for (signal, wait) in [
                (libc::SIGHUP, AFTER_HANGUP),
                (libc::SIGTERM, grace),
                (libc::SIGKILL, Duration::ZERO),
            ] {
                {
                    let state = state.lock().unwrap_or_else(|e| e.into_inner());
                    if send(&state, signal).is_err() {
                        return;
                    }
                }
                tokio::time::sleep(wait).await;
            }
        });
        Ok(())
    }

    /// The screen's text, after up to `lines` lines of history, once the
    /// actor answers. The request is made at once, so that no lock is held
    /// while it is awaited.
    pub fn text_request(&self, lines: usize) -> impl Future<Output = Option<String>> + use<> {
        let (reply, answer) = oneshot::channel();
        let sent = self.commands.send(Command::Text { lines, reply }).is_ok();
        async move {
            if !sent {
                return None;
            }
            answer.await.ok()
        }
    }
}

/// Sends a signal under the state's lock.
fn send(state: &State, signal: i32) -> Result<(), SignalError> {
    let Some(master) = &state.master else {
        return Err(SignalError::Exited);
    };
    let pid = state.info.pid;
    pty::signal_group(pid, signal).map_err(SignalError::Io)?;
    if let Some(foreground) = pty::foreground_group(master)
        && foreground != pid
    {
        let _ = pty::signal_group(foreground, signal);
    }
    Ok(())
}

struct Actor {
    id: String,
    screen: Box<dyn Screen>,
    state: Arc<Mutex<State>>,
    /// To the writer thread; dropped when the terminal ends.
    input: Option<std::sync::mpsc::Sender<Vec<u8>>>,
    /// Written to stop the reader; dropped when the terminal ends.
    wake: Option<io::PipeWriter>,
    pid: u32,
    output: mpsc::Receiver<Output>,
    commands: mpsc::UnboundedReceiver<Command>,
    context: Context,
    /// The last title reported, without a leading spinner.
    title: Option<String>,
}

/// Applies output and commands in the order they come, until the terminal
/// has ended and nothing refers to it any more.
async fn actor(mut actor: Actor) {
    let mut exit = None;
    let mut reader_done = false;
    let mut drain_until = Instant::now();
    let mut ended = false;
    loop {
        let draining = exit.is_some() && !ended;
        tokio::select! {
            output = actor.output.recv(), if !reader_done => match output {
                Some(Output::Bytes(bytes)) => {
                    actor.apply(&bytes);
                    drain_until = Instant::now() + EXIT_DRAIN;
                }
                Some(Output::Done) | None => reader_done = true,
            },
            command = actor.commands.recv() => match command {
                Some(Command::Exited(status)) => {
                    exit = Some(status);
                    drain_until = Instant::now() + EXIT_DRAIN;
                }
                Some(Command::Text { lines, reply }) => {
                    let _ = reply.send(actor.text(lines));
                }
                None => break,
            },
            () = tokio::time::sleep_until(drain_until), if draining => {
                reader_done = true;
            }
        }
        if let Some(status) = exit
            && !ended
            && reader_done
        {
            actor.end(status);
            ended = true;
        }
    }
}

/// Characters an agent spins at the start of its title.
fn is_spinner(c: char) -> bool {
    ('\u{2800}'..='\u{28ff}').contains(&c) || "✳✶✻✽✢·*⠂⠐◐◓◑◒|/-\\".contains(c) || c.is_whitespace()
}

/// Text cut to `max` characters.
fn trimmed(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

impl Actor {
    fn apply(&mut self, bytes: &[u8]) {
        let (mut replies, mut signals) = (Vec::new(), Vec::<Signal>::new());
        self.screen.feed(bytes, &mut replies, &mut signals);
        if !replies.is_empty()
            && let Some(input) = &self.input
        {
            let _ = input.send(replies);
        }
        for signal in signals {
            self.report(signal);
        }
    }

    /// Reports what the program told its terminal as an event.
    fn report(&mut self, signal: Signal) {
        let events = &self.context.events;
        let terminal = &self.id;
        match signal {
            Signal::Title(text) => {
                // a spinning title is one title
                let key = text.trim_start_matches(is_spinner).to_string();
                if self.title.as_ref() != Some(&key) {
                    self.title = Some(key);
                    events.emit("title", json!({"terminal": terminal, "text": text}));
                }
            }
            Signal::Bell => events.emit("bell", json!({"terminal": terminal})),
            Signal::Notify { title, body } => events.emit(
                "notify",
                json!({
                    "terminal": terminal,
                    "title": title.map(|t| trimmed(&t, 80)),
                    "body": trimmed(&body, 240),
                }),
            ),
            Signal::ProgramStatus(status) => {
                let mut params = json!({
                    "terminal": terminal,
                    "state": format!("{:?}", status.state).to_lowercase(),
                });
                if let Some(wait) = status.wait {
                    params["kind"] = json!(format!("{wait:?}").to_lowercase());
                }
                if !status.id.is_empty() {
                    params["id"] = json!(status.id);
                }
                events.emit("program_status", params);
            }
            // reported to viewers with the focus work
            Signal::FocusReporting(_) => {}
        }
    }

    fn text(&self, lines: usize) -> String {
        let history = self.screen.text(TextRange::History);
        let kept: Vec<&str> = history.lines().collect();
        let mut text: String = kept[kept.len().saturating_sub(lines)..]
            .iter()
            .map(|line| format!("{line}\n"))
            .collect();
        text.push_str(&self.screen.text(TextRange::Screen));
        text
    }

    /// Records the exit, stops the reader and the writer, reaps the
    /// program, which nothing signals any more, keeps the final screen and
    /// reports the exit.
    fn end(&mut self, status: Exit) {
        let (reason, ended_at) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let reason = state.stopping.unwrap_or(match status {
                Exit::Signal(_) => Reason::Signaled,
                Exit::Code(_) => Reason::Exited,
            });
            let ended_at = crate::now();
            state.info.ended_at = Some(ended_at.clone());
            state.info.exit = Some(status);
            state.master = None;
            pty::reap(self.pid);
            (reason, ended_at)
        };
        if let Some(mut wake) = self.wake.take() {
            let _ = wake.write_all(b"x");
        }
        self.input = None;
        let _ = keep_screen(
            &self.context.screens,
            &self.id,
            &self.screen.first_paint(FINAL_HISTORY),
        );
        self.context.records.update(&self.id, |record| {
            record["ended_at"] = json!(ended_at);
            record["exit"] = exit_json(status);
        });
        let mut params = json!({"terminal": self.id, "reason": reason.as_str()});
        match status {
            Exit::Code(code) => params["code"] = json!(code),
            Exit::Signal(signal) => params["signal"] = json!(signal),
        }
        self.context.events.emit("exited", params);
    }
}

/// The lines of history kept with a final screen.
const FINAL_HISTORY: usize = 500;

/// Writes a terminal's final screen, readable by its owner only.
fn keep_screen(folder: &Path, id: &str, paint: &[u8]) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::create_dir_all(folder)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(folder.join(format!("{id}.snap")))?;
    file.write_all(paint)
}

/// Reads the PTY until it ends or the wake descriptor is written. Reads
/// first, and waits in `poll` only when a read would block.
fn read_pty(mut master: File, wake: io::PipeReader, output: mpsc::Sender<Output>) {
    let mut buf = vec![0u8; CHUNK];
    loop {
        match master.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if output
                    .blocking_send(Output::Bytes(buf[..n].to_vec()))
                    .is_err()
                {
                    return;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                match pty::wait_readable(&master, &wake) {
                    Ok(Ready::Master) => {}
                    Ok(Ready::Wake) | Err(_) => break,
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            // EIO once every slave descriptor is closed
            Err(_) => break,
        }
    }
    let _ = output.blocking_send(Output::Done);
}

/// Writes every input to the PTY in order, waiting when it is full, until
/// the terminal ends.
fn write_pty(mut master: File, input: std::sync::mpsc::Receiver<Vec<u8>>) {
    for bytes in input {
        let mut rest = &bytes[..];
        while !rest.is_empty() {
            match master.write(rest) {
                Ok(n) => rest = &rest[n..],
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if pty::wait_writable(&master).is_err() {
                        return;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return,
            }
        }
    }
}

/// A new screen for a terminal of this size.
pub fn screen(cols: u16, rows: u16, scrollback_lines: usize) -> io::Result<Box<dyn Screen>> {
    pinrail_screen::ghostty::Terminal::new(&Options {
        cols,
        rows,
        scrollback_lines,
    })
    .map(|t| Box::new(t) as Box<dyn Screen>)
    .map_err(io::Error::other)
}
