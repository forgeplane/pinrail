//! One terminal: a reader thread that reads the PTY, an actor task that
//! owns the screen and applies everything in one order, a writer thread
//! that writes to the PTY, and a waiter thread that sees the program exit.

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pinrail_screen::{Options, Screen, Signal, TextRange};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use crate::events::Events;
use crate::leader::{self, Leader};
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

/// Where input to a terminal comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// A key, paste or mouse click the person sent from a viewer.
    Person,
    /// What a viewer's own emulator answered to a query, which is not the
    /// person's input.
    Reply,
}

/// A message to type into the program, with `terminal.submit`'s guards.
#[derive(Debug, Clone)]
pub struct Submit {
    pub text: String,
    /// Whether to type the text inside a bracketed paste.
    pub bracketed: bool,
    /// How long the output must be quiet after the paste before the
    /// submit keys.
    pub settle: Duration,
    /// The longest wait after the paste.
    pub max: Duration,
    /// In the program's first 10 seconds, how long its output must be
    /// quiet before the paste; zero turns the rule off.
    pub startup_quiet: Duration,
    /// The keys that submit, such as a carriage return.
    pub submit: String,
    /// The process that must hold the terminal.
    pub expect_pid: Option<u32>,
    /// When the agent last became idle; input since then refuses.
    pub idle_since: Option<String>,
    /// Types even if the person used the terminal since `idle_since`.
    pub anyway: bool,
}

/// Why a message was not typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitError {
    /// The program has exited.
    Exited,
    /// Another process holds the terminal.
    NotForeground,
    /// The person used the terminal since the agent became idle.
    UsedSinceIdle,
    /// Another message is being typed.
    Busy,
}

impl SubmitError {
    pub fn code(self) -> &'static str {
        match self {
            SubmitError::Exited => "exited",
            SubmitError::NotForeground => "not_foreground",
            SubmitError::UsedSinceIdle => "used_since_idle",
            SubmitError::Busy => "busy",
        }
    }
}

/// How long after the program starts the startup rule applies.
const STARTUP: Duration = Duration::from_secs(10);

/// A message being typed.
struct Typing {
    submit: Submit,
    reply: oneshot::Sender<Result<(), SubmitError>>,
    /// When the paste was written, once it was.
    pasted_at: Option<Instant>,
}

enum Command {
    Exited(Exit),
    Input(Vec<u8>, Input),
    Submit(Submit, oneshot::Sender<Result<(), SubmitError>>),
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
    /// When the person last typed, pasted or clicked in the terminal.
    last_input_at: Option<String>,
}

/// What a terminal reports to and keeps for the rest of the host.
#[derive(Clone)]
pub struct Context {
    pub events: Arc<Events>,
    pub records: Arc<Records>,
    /// Where the final screen is kept.
    pub screens: PathBuf,
    /// Whether a controller is connected, which makes the foreground
    /// process read more often.
    pub controlled: Arc<AtomicBool>,
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
            last_input_at: None,
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
            foreground: None,
            in_paste: false,
            started: Instant::now(),
            last_output: Instant::now(),
            typing: None,
            held: Vec::new(),
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

    /// Writes input to the program, in order with everything else the
    /// actor does.
    pub fn input(&self, bytes: Vec<u8>, from: Input) {
        let _ = self.commands.send(Command::Input(bytes, from));
    }

    /// Types a message, once its guards pass, and answers when the submit
    /// keys were written.
    pub fn submit(&self, submit: Submit) -> impl Future<Output = Result<(), SubmitError>> + use<> {
        let (reply, answer) = oneshot::channel();
        let sent = self.commands.send(Command::Submit(submit, reply)).is_ok();
        async move {
            if !sent {
                return Err(SubmitError::Exited);
            }
            answer.await.unwrap_or(Err(SubmitError::Exited))
        }
    }

    /// When the person last typed, pasted or clicked in the terminal.
    pub fn last_input_at(&self) -> Option<String> {
        self.lock().last_input_at.clone()
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
    /// The foreground process last reported.
    foreground: Option<Leader>,
    /// Whether the person's input is inside a bracketed paste.
    in_paste: bool,
    started: Instant,
    /// When the program last wrote output.
    last_output: Instant,
    typing: Option<Typing>,
    /// The person's input held while a message is typed.
    held: Vec<Vec<u8>>,
}

/// Applies output and commands in the order they come, until the terminal
/// has ended and nothing refers to it any more.
async fn actor(mut actor: Actor) {
    let mut exit = None;
    let mut reader_done = false;
    let mut drain_until = Instant::now();
    let mut ended = false;
    let mut next_look = Instant::now();
    loop {
        let draining = exit.is_some() && !ended;
        let typing_at = actor.typing_due();
        tokio::select! {
            () = tokio::time::sleep_until(typing_at.unwrap_or(next_look)), if typing_at.is_some() => {
                actor.type_on();
            }
            () = tokio::time::sleep_until(next_look), if !ended => {
                actor.look_at_foreground();
                let controlled = actor.context.controlled.load(Ordering::Relaxed);
                next_look = Instant::now() + if controlled { LOOK_CONTROLLED } else { LOOK_IDLE };
            }
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
                Some(Command::Input(bytes, from)) => actor.input(bytes, from),
                Some(Command::Submit(submit, reply)) => {
                    if ended {
                        let _ = reply.send(Err(SubmitError::Exited));
                    } else {
                        actor.start_typing(submit, reply);
                    }
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

/// The bytes of a message's paste: the text, inside a bracketed paste when
/// asked, with every end-of-paste marker removed from it, so that the text
/// cannot end the paste early and have its rest read as typed keys.
fn paste(submit: &Submit) -> Vec<u8> {
    if !submit.bracketed {
        return submit.text.clone().into_bytes();
    }
    let mut bytes = b"\x1b[200~".to_vec();
    bytes.extend(submit.text.replace("\x1b[201~", "").into_bytes());
    bytes.extend(b"\x1b[201~");
    bytes
}

/// Whether the person's last input came after `idle_since`.
fn used_since(last_input_at: &Option<String>, idle_since: &Option<String>) -> bool {
    let parse = |t: &Option<String>| {
        t.as_deref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
    };
    match (parse(last_input_at), parse(idle_since)) {
        (Some(input), Some(idle)) => input > idle,
        // input with no idle time to compare is not since then
        _ => false,
    }
}

/// How often the foreground process is read while a controller is
/// connected, and otherwise.
const LOOK_CONTROLLED: Duration = Duration::from_millis(500);
const LOOK_IDLE: Duration = Duration::from_secs(2);

/// Whether the person's input is an interrupt: a Ctrl-C or an Escape on its
/// own, not the start of a sequence and not inside a paste. Tracks whether
/// the input is inside a bracketed paste.
fn is_interrupt(bytes: &[u8], in_paste: &mut bool) -> bool {
    let was_in_paste = *in_paste;
    if let Some(start) = find(bytes, b"\x1b[200~") {
        *in_paste = find(&bytes[start..], b"\x1b[201~").is_none();
    } else if find(bytes, b"\x1b[201~").is_some() {
        *in_paste = false;
    }
    !was_in_paste && (bytes == b"\x03" || bytes == b"\x1b")
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
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
    /// Writes input to the program; the person's input counts as activity,
    /// and a standalone interrupt is reported.
    fn input(&mut self, bytes: Vec<u8>, from: Input) {
        if from == Input::Person && self.typing.is_some() {
            // held, so the person's keys cannot land between the paste and
            // the submit keys
            self.state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .last_input_at = Some(crate::now());
            self.held.push(bytes);
            return;
        }
        if from == Input::Person {
            let interrupt = is_interrupt(&bytes, &mut self.in_paste);
            self.state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .last_input_at = Some(crate::now());
            if interrupt {
                self.context
                    .events
                    .emit("input", json!({"terminal": self.id, "kind": "interrupt"}));
            }
        }
        if let Some(input) = &self.input {
            let _ = input.send(bytes);
        }
    }

    /// Reads the PTY's foreground process group's leader, and reports it
    /// when it changed.
    fn look_at_foreground(&mut self) {
        let group = {
            let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.master.as_ref().and_then(pty::foreground_group)
        };
        let leader = group.and_then(leader::read);
        // another process or another program: arguments and folder alone
        // do not count, and may not be readable yet right after an exec
        let key = |l: &Option<Leader>| l.as_ref().map(|l| (l.pid, l.name.clone()));
        if leader.is_some() && key(&leader) != key(&self.foreground) {
            self.foreground = leader.clone();
            if let Some(leader) = leader {
                let mut params = leader.to_json();
                params["terminal"] = json!(self.id);
                self.context.events.emit("foreground", params);
            }
        }
    }

    /// Checks the guards and starts typing a message.
    fn start_typing(&mut self, submit: Submit, reply: oneshot::Sender<Result<(), SubmitError>>) {
        let refused = if self.typing.is_some() {
            Some(SubmitError::Busy)
        } else {
            let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let foreground = state.master.as_ref().and_then(pty::foreground_group);
            if state.master.is_none() {
                Some(SubmitError::Exited)
            } else if submit.expect_pid.is_some() && foreground != submit.expect_pid {
                Some(SubmitError::NotForeground)
            } else if !submit.anyway && used_since(&state.last_input_at, &submit.idle_since) {
                Some(SubmitError::UsedSinceIdle)
            } else {
                None
            }
        };
        if let Some(error) = refused {
            let _ = reply.send(Err(error));
            return;
        }
        self.typing = Some(Typing {
            submit,
            reply,
            pasted_at: None,
        });
        self.type_on();
    }

    /// When the message being typed can go on, if one is.
    fn typing_due(&self) -> Option<Instant> {
        let typing = self.typing.as_ref()?;
        let submit = &typing.submit;
        Some(match typing.pasted_at {
            // wait for a starting program to go quiet, within its first
            // 10 seconds
            None => {
                let startup_ends = self.started + STARTUP;
                if submit.startup_quiet.is_zero() {
                    Instant::now()
                } else {
                    (self.last_output + submit.startup_quiet).min(startup_ends)
                }
            }
            // after the paste, wait for the output to settle, or `max`
            Some(pasted_at) => {
                ((self.last_output.max(pasted_at)) + submit.settle).min(pasted_at + submit.max)
            }
        })
    }

    /// Writes the paste or the submit keys, when their time has come.
    fn type_on(&mut self) {
        let Some(due) = self.typing_due() else {
            return;
        };
        if Instant::now() < due {
            return;
        }
        let Some(typing) = self.typing.as_mut() else {
            return;
        };
        let Some(input) = &self.input else {
            return;
        };
        if typing.pasted_at.is_none() {
            let _ = input.send(paste(&typing.submit));
            typing.pasted_at = Some(Instant::now());
            return;
        }
        let _ = input.send(typing.submit.submit.clone().into_bytes());
        for held in self.held.drain(..) {
            let _ = input.send(held);
        }
        if let Some(typing) = self.typing.take() {
            let _ = typing.reply.send(Ok(()));
        }
    }

    fn apply(&mut self, bytes: &[u8]) {
        self.last_output = Instant::now();
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
        if let Some(typing) = self.typing.take() {
            let _ = typing.reply.send(Err(SubmitError::Exited));
        }
        self.held.clear();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_standalone_ctrl_c_or_escape_is_an_interrupt() {
        let mut in_paste = false;
        assert!(is_interrupt(b"\x03", &mut in_paste));
        assert!(is_interrupt(b"\x1b", &mut in_paste));
        // the start of a sequence, or a key with others
        assert!(!is_interrupt(b"\x1b[A", &mut in_paste));
        assert!(!is_interrupt(b"ab\x03", &mut in_paste));
        assert!(!is_interrupt(b"q", &mut in_paste));
    }

    fn submit(text: &str, bracketed: bool) -> Submit {
        Submit {
            text: text.to_string(),
            bracketed,
            settle: Duration::ZERO,
            max: Duration::ZERO,
            startup_quiet: Duration::ZERO,
            submit: "\r".to_string(),
            expect_pid: None,
            idle_since: None,
            anyway: false,
        }
    }

    #[test]
    fn a_bracketed_paste_cannot_be_ended_by_its_text() {
        assert_eq!(
            paste(&submit("a\x1b[201~b\x1b[201~c", true)),
            b"\x1b[200~abc\x1b[201~"
        );
        assert_eq!(paste(&submit("plain", false)), b"plain");
    }

    #[test]
    fn input_after_the_agent_became_idle_counts() {
        let at = |t: &str| Some(t.to_string());
        assert!(used_since(
            &at("2026-11-13T10:00:05.000000Z"),
            &at("2026-11-13T10:00:00Z")
        ));
        assert!(!used_since(
            &at("2026-11-13T09:59:00.000000Z"),
            &at("2026-11-13T10:00:00Z")
        ));
        assert!(!used_since(&None, &at("2026-11-13T10:00:00Z")));
        assert!(!used_since(&at("2026-11-13T10:00:05Z"), &None));
    }

    #[test]
    fn nothing_inside_a_paste_is_an_interrupt() {
        let mut in_paste = false;
        assert!(!is_interrupt(b"\x1b[200~first part", &mut in_paste));
        assert!(in_paste);
        assert!(!is_interrupt(b"\x03", &mut in_paste));
        assert!(!is_interrupt(b"rest\x1b[201~", &mut in_paste));
        assert!(!in_paste);
        assert!(is_interrupt(b"\x03", &mut in_paste));
        // a paste that starts and ends in one chunk
        assert!(!is_interrupt(b"\x1b[200~x\x1b[201~", &mut in_paste));
        assert!(!in_paste);
    }
}
