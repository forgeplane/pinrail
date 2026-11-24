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

use pinrail_screen::{ColorScheme, DisplayFacts, Options, Rgb, Screen, Signal, TextRange};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use crate::events::Events;
use crate::filter::QueryFilter;
use crate::frame::{Frame, kind};
use crate::leader::{self, Leader};
use crate::outbox::Outbox;
use crate::pty::{self, Exit, Ready, Spawned};
use crate::records::Records;
use crate::viewers::{self, AttachRequest, ViewerId, Viewers};

/// The most the reader reads at once.
const CHUNK: usize = 64 * 1024;
/// Chunks waiting for the actor before the reader stops reading, which
/// holds at most 8 MiB, so a runaway program blocks as it would on a PTY
/// nobody reads.
const QUEUED_CHUNKS: usize = 8 * 1024 * 1024 / CHUNK;
/// How much of the screen and its history is kept when a stop is asked
/// for.
const KEPT_LINES: usize = 500;
/// How long a stop waits for the screen to be kept before it signals.
const KEEP_WITHIN: Duration = Duration::from_millis(200);
/// How soon before its exit a program may have left the alternate screen
/// for that screen to be the one it ended with.
const LEFT_WITHIN: Duration = Duration::from_secs(3);

/// A screen kept as the one a terminal ended with: its text, its paint
/// any emulator reads, and its size.
#[derive(Debug, Clone)]
struct Kept {
    text: String,
    paint: Vec<u8>,
    size: (u16, u16),
}

/// Where `bytes` leave the alternate screen (modes 1049, 1047 or 47), in
/// one pass over the escape characters.
fn leaves_alternate_at(bytes: &[u8]) -> Option<usize> {
    const LEAVE: [&[u8]; 3] = [b"\x1b[?1049l", b"\x1b[?1047l", b"\x1b[?47l"];
    let mut from = 0;
    while let Some(i) = bytes[from..].iter().position(|&b| b == 0x1b) {
        let at = from + i;
        if LEAVE
            .iter()
            .any(|sequence| bytes[at..].starts_with(sequence))
        {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

/// Feeds program output to `screen`. When the output leaves the alternate
/// screen, `left` sees the alternate screen as it was just before.
pub fn feed_output(
    screen: &mut dyn Screen,
    bytes: &[u8],
    replies: &mut Vec<u8>,
    signals: &mut Vec<Signal>,
    left: impl FnOnce(&dyn Screen),
) {
    let mut rest = bytes;
    // output on the primary screen, most of it, is not searched
    if screen.modes().alternate_screen
        && let Some(at) = leaves_alternate_at(rest)
    {
        screen.feed(&rest[..at], replies, signals);
        left(screen);
        rest = &rest[at..];
    }
    screen.feed(rest, replies, signals);
}

#[cfg(test)]
mod alternate_tests {
    use super::leaves_alternate_at;

    #[test]
    fn the_first_of_the_three_ways_to_leave_the_alternate_screen_is_found() {
        assert_eq!(
            leaves_alternate_at(b"text\x1b[?47l more \x1b[?1049l"),
            Some(4)
        );
        assert_eq!(leaves_alternate_at(b"\x1b[1m\x1b[?1047l"), Some(4));
        assert_eq!(
            leaves_alternate_at(b"\x1b[?1049h entered, not left \x1b"),
            None
        );
    }
}

/// How long the actor waits for the last output after the program exits.
/// The count starts again with each chunk that arrives, so an exit never
/// overtakes its output.
const EXIT_DRAIN: Duration = Duration::from_millis(200);

/// What the core and `terminal.list` see of a terminal.
#[derive(Debug, Clone, PartialEq)]
pub struct Info {
    pub id: String,
    pub pid: u32,
    /// The program's start time, as `pinrail-runtime` reads it, which
    /// tells the core this process from a later one with the same pid.
    pub process_started: Option<String>,
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
            "process_started": self.process_started,
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
    /// Keys typed with the paste, before it, that clear the program's input.
    pub clear: String,
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
    /// When the keys that clear the input were written, once they were.
    cleared_at: Option<Instant>,
    /// When the paste was written, once it was.
    pasted_at: Option<Instant>,
}

/// Why a viewer could not attach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachError {
    /// The program has exited; its final screen is in `terminal.text`.
    Exited,
    /// None of the snapshot formats the viewer reads.
    Unsupported,
}

enum Command {
    Exited(Exit),
    /// Keeps the screen as it is now, as the one the terminal ended with.
    Keep(oneshot::Sender<()>),
    Focus(bool),
    Attach {
        viewer: ViewerId,
        out: Outbox,
        request: AttachRequest,
        reply: oneshot::Sender<Result<String, AttachError>>,
    },
    Resume(ViewerId),
    Channel(ViewerId, Frame),
    Detach(ViewerId),
    Input(Vec<u8>, Input),
    Submit(Submit, oneshot::Sender<Result<(), SubmitError>>),
    Text {
        lines: usize,
        reply: oneshot::Sender<String>,
    },
    /// The screen as a paint any emulator reads, with its size.
    Paint(oneshot::Sender<(Vec<u8>, (u16, u16))>),
    HandOver {
        file: std::path::PathBuf,
        formats: Vec<String>,
        reply: oneshot::Sender<io::Result<()>>,
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
    /// Set once the exit is recorded and its event is sent.
    exit_reported: bool,
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

/// What the holder keeps for a terminal, as the terminal sees it.
pub struct Holding {
    /// Runs once the terminal has ended and nothing signals the program
    /// any more, for the holder to forget it and reap the program.
    pub forget: Box<dyn FnOnce() + Send>,
    /// The output ring in shared memory, which the holder keeps too.
    pub ring: Option<crate::ring::SharedRing>,
    /// The bytes of an unfinished sequence that the previous host's query
    /// filter held back, after a hand-over.
    pub held: Vec<u8>,
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
    /// Starts reading a terminal whose program has been spawned. The
    /// program's exit comes through [`Terminal::exited`].
    pub fn start(
        id: String,
        spawned: Spawned,
        labels: Value,
        (cols, rows): (u16, u16),
        screen: Box<dyn Screen>,
        context: Context,
        holding: Holding,
    ) -> io::Result<Self> {
        let Holding { forget, ring, held } = holding;
        // the sequence the previous host held stays held, unfed, until it
        // is complete
        let mut filter = QueryFilter::default();
        filter.push(&held, &mut Vec::new(), &mut |_| 0);
        let pid = spawned.pid;
        let state = Arc::new(Mutex::new(State {
            info: Info {
                id: id.clone(),
                pid,
                process_started: spawned.started.clone(),
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
            exit_reported: false,
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
        tokio::spawn(actor(Actor {
            id,
            screen,
            state: state.clone(),
            input: Some(input_tx),
            wake: Some(wake_write),
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
            filter,
            for_viewers: Vec::new(),
            viewers: Viewers::with_shared(ring),
            size: (cols, rows),
            ended: false,
            focus_reporting: false,
            focused: false,
            kept: None,
            left_alternate: None,
            forget: Some(forget),
        }));
        Ok(Self { state, commands })
    }

    /// Marks the terminal as adopted after a crash: input from before it
    /// is unknown, so it counts as just now, and the input guard holds a
    /// message until the agent is next idle.
    pub fn adopted(&self) {
        self.adopted_with(None);
    }

    /// Marks the terminal as adopted, with the last input a hand-over
    /// recorded, or now when there is none.
    pub fn adopted_with(&self, last_input_at: Option<String>) {
        self.lock().last_input_at = Some(last_input_at.unwrap_or_else(crate::now));
    }

    /// Hands the terminal over to the next host: stops reading the PTY,
    /// applies what was read, and writes the screen to `file` in the first
    /// of `formats` it can produce. The program's later output waits in
    /// the PTY for the next host. Answers once the file is written.
    pub fn hand_over(
        &self,
        file: std::path::PathBuf,
        formats: Vec<String>,
    ) -> oneshot::Receiver<io::Result<()>> {
        let (reply, answer) = oneshot::channel();
        let _ = self.commands.send(Command::HandOver {
            file,
            formats,
            reply,
        });
        answer
    }

    /// The program exited, as its holder reports.
    pub fn exited(&self, exit: Exit) {
        let _ = self.commands.send(Command::Exited(exit));
    }

    /// Reports the program's exit, for a program spawned without a holder,
    /// such as a test's. It does not keep the terminal running.
    pub fn exit_reporter(&self) -> impl Fn(Exit) + Send + 'static {
        let commands = self.commands.downgrade();
        move |exit| {
            if let Some(commands) = commands.upgrade() {
                let _ = commands.send(Command::Exited(exit));
            }
        }
    }

    pub fn info(&self) -> Info {
        self.lock().info.clone()
    }

    pub fn live(&self) -> bool {
        self.lock().info.ended_at.is_none()
    }

    /// Whether the terminal has ended and said so: its final screen is
    /// kept, its record has the exit, and the `exited` event is sent. A
    /// terminal stops being live a moment before.
    pub fn exit_reported(&self) -> bool {
        self.lock().exit_reported
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
        let commands = self.commands.clone();
        tokio::spawn(async move {
            // the screen the person saw, before the program clears it on
            // its way out
            let (kept, done) = oneshot::channel();
            if commands.send(Command::Keep(kept)).is_ok() {
                let _ = tokio::time::timeout(KEEP_WITHIN, done).await;
            }
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

    /// Opens a viewer on the terminal, paused until [`Terminal::resume`]:
    /// answers with the snapshot format it gets.
    pub fn attach(
        &self,
        viewer: ViewerId,
        out: Outbox,
        request: AttachRequest,
    ) -> impl Future<Output = Result<String, AttachError>> + use<> {
        let (reply, answer) = oneshot::channel();
        let sent = self
            .commands
            .send(Command::Attach {
                viewer,
                out,
                request,
                reply,
            })
            .is_ok();
        async move {
            if !sent {
                return Err(AttachError::Exited);
            }
            answer.await.unwrap_or(Err(AttachError::Exited))
        }
    }

    /// Starts sending to a viewer, once its attach reply has been queued.
    pub fn resume(&self, viewer: ViewerId) {
        let _ = self.commands.send(Command::Resume(viewer));
    }

    /// A frame a viewer sent on its terminal channel.
    pub fn channel(&self, viewer: ViewerId, frame: Frame) {
        let _ = self.commands.send(Command::Channel(viewer, frame));
    }

    pub fn detach(&self, viewer: ViewerId) {
        let _ = self.commands.send(Command::Detach(viewer));
    }

    /// Whether the controlling viewer's window is focused and shows the
    /// terminal, which a program that asked for focus reports is told.
    pub fn set_focus(&self, focused: bool) {
        let _ = self.commands.send(Command::Focus(focused));
    }

    /// When the person last typed, pasted or clicked in the terminal.
    pub fn last_input_at(&self) -> Option<String> {
        self.lock().last_input_at.clone()
    }

    /// The screen's text, after up to `lines` lines of history, once the
    /// actor answers. The request is made at once, so that no lock is held
    /// while it is awaited.
    /// The screen as a paint any emulator reads, with its columns and
    /// rows: the one it ended with, once it has.
    pub fn paint_request(&self) -> impl Future<Output = Option<(Vec<u8>, (u16, u16))>> + use<> {
        let (reply, answer) = oneshot::channel();
        let sent = self.commands.send(Command::Paint(reply)).is_ok();
        async move {
            if !sent {
                return None;
            }
            answer.await.ok()
        }
    }

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
    filter: QueryFilter,
    /// The last output as the viewers see it, without the answered
    /// queries.
    for_viewers: Vec<u8>,
    /// Whether the program asked for focus reports (mode 1004).
    focus_reporting: bool,
    /// Whether a controlling viewer's window is focused and shows the
    /// terminal.
    focused: bool,
    viewers: Viewers,
    /// The PTY's size, as columns and rows.
    size: (u16, u16),
    ended: bool,
    /// The screen the terminal ended with, when it is not the one the
    /// program left: the screen when it was asked to stop, or the
    /// alternate screen it left as it exited.
    kept: Option<Kept>,
    /// The alternate screen the program last left, and when.
    left_alternate: Option<(Kept, Instant)>,
    /// Tells the holder the host is done with the terminal.
    forget: Option<Box<dyn FnOnce() + Send>>,
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
            // a burst of output goes out to the viewers 2 ms after it began
            () = tokio::time::sleep_until(
                actor.viewers.gathered_due().map_or(next_look, Instant::from_std),
            ),
                if actor.viewers.gathered_due().is_some() =>
            {
                actor.viewers.send_gathered();
            }
            () = tokio::time::sleep_until(next_look), if !ended => {
                actor.viewers.prune();
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
                Some(Command::Focus(focused)) => actor.set_focus(focused),
                Some(Command::Attach { viewer, out, request, reply }) => {
                    let _ = reply.send(actor.attach(viewer, out, &request));
                }
                Some(Command::Resume(viewer)) => actor.with_paint(|viewers, paint, size| {
                    viewers.resume(viewer, paint, size);
                }),
                Some(Command::Channel(viewer, frame)) => actor.channel(viewer, frame),
                Some(Command::Detach(viewer)) => actor.detach(viewer),
                Some(Command::Submit(submit, reply)) => {
                    if ended {
                        let _ = reply.send(Err(SubmitError::Exited));
                    } else {
                        actor.start_typing(submit, reply);
                    }
                }
                Some(Command::Keep(done)) => {
                    if actor.kept.is_none() {
                        actor.kept = Some(actor.keep());
                    }
                    let _ = done.send(());
                }
                Some(Command::Paint(reply)) => {
                    let _ = reply.send(actor.paint());
                }
                Some(Command::Text { lines, reply }) => {
                    let _ = reply.send(actor.text(lines));
                }
                Some(Command::HandOver { file, formats, reply }) => {
                    // the reader stops; what it read still counts
                    if let Some(mut wake) = actor.wake.take() {
                        let _ = wake.write_all(b"x");
                    }
                    while !reader_done {
                        match actor.output.recv().await {
                            Some(Output::Bytes(bytes)) => actor.apply(&bytes),
                            Some(Output::Done) | None => reader_done = true,
                        }
                    }
                    actor.viewers.send_gathered();
                    let _ = reply.send(actor.write_hand_over(&file, &formats));
                    // the holder keeps the program: nothing is forgotten
                    actor.forget = None;
                    break;
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
    const END_PASTE: &str = "\x1b[201~";
    if !submit.bracketed {
        return submit.text.clone().into_bytes();
    }
    // removing a marker can join the text around it into another one
    let mut text = submit.text.clone();
    while text.contains(END_PASTE) {
        text = text.replace(END_PASTE, "");
    }
    let mut bytes = b"\x1b[200~".to_vec();
    bytes.extend(text.into_bytes());
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
        if self.stopping() {
            return;
        }
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

    /// Tells a program that asked for focus reports whether the terminal
    /// now has a focused controlling viewer.
    fn set_focus(&mut self, focused: bool) {
        if focused != self.focused {
            self.focused = focused;
            if self.focus_reporting {
                self.write_focus();
            }
        }
    }

    /// Whether the program was asked to stop. Nothing is written to it
    /// then: it may already have given the terminal back to line mode,
    /// which would echo what it is sent, such as a focus report, onto
    /// its last screen.
    fn stopping(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stopping
            .is_some()
    }

    fn write_focus(&self) {
        if self.stopping() {
            return;
        }
        if let Some(input) = &self.input {
            let report = if self.focused { b"\x1b[I" } else { b"\x1b[O" };
            let _ = input.send(report.to_vec());
        }
    }

    /// Runs `act` with the viewers and a function that paints the screen
    /// for them, or `None` while the program is inside an escape sequence
    /// too long for a snapshot to reproduce.
    fn with_paint(
        &mut self,
        act: impl FnOnce(&mut Viewers, &dyn Fn(usize) -> Option<Vec<u8>>, (u16, u16)),
    ) {
        let screen = &self.screen;
        // the bytes the filter holds are not on the screen yet, and every
        // viewer gets them in the stream if they pass the filter
        let paint = |history: usize| screen.paintable().then(|| screen.first_paint(history));
        act(&mut self.viewers, &paint, self.size);
    }

    /// Opens a viewer: the output it missed when it resumes and the ring
    /// still holds it, otherwise a snapshot in the first format it reads
    /// that the screen can produce.
    fn attach(
        &mut self,
        viewer: ViewerId,
        out: Outbox,
        request: &AttachRequest,
    ) -> Result<String, AttachError> {
        if self.ended {
            return Err(AttachError::Exited);
        }
        let channel = viewer.1;
        let offset = self.viewers.offset();
        let (format, frames) = if let Some(missed) = request
            .resume_from
            .and_then(|from| self.viewers.since(from))
        {
            let mut frames = vec![viewers::meta(
                channel,
                json!({"type": "resume", "offset": request.resume_from}),
            )];
            frames.extend(
                viewers::snapshot_frames(channel, "resume", self.size, offset, &[], &missed)
                    .split_off(2),
            );
            ("resume".to_string(), frames)
        } else if !self.screen.paintable() {
            // inside a long escape sequence: the snapshot waits for its end
            self.viewers
                .add_waiting(viewer, out, (request.cols, request.rows), request.history);
            return Ok("vt".to_string());
        } else {
            let binary = self.screen.binary_snapshot();
            let wanted = |format: &str| {
                request.formats.is_empty() || request.formats.iter().any(|f| f == format)
            };
            let chosen = request.formats.iter().find_map(|format| match &binary {
                Some(snapshot) if *format == snapshot.format => Some(format.clone()),
                _ if format == "vt" => Some(format.clone()),
                _ => None,
            });
            let format = match chosen {
                Some(format) => format,
                None if wanted("vt") => "vt".to_string(),
                None => return Err(AttachError::Unsupported),
            };
            let frames = match binary {
                Some(snapshot) if snapshot.format == format => viewers::snapshot_frames(
                    channel,
                    &format,
                    self.size,
                    offset,
                    &snapshot.bytes,
                    &[],
                ),
                _ => {
                    let paint = self.screen.first_paint(request.history);
                    viewers::snapshot_frames(channel, "vt", self.size, offset, &paint, &[])
                }
            };
            (format, frames)
        };
        self.viewers
            .add(viewer, out, (request.cols, request.rows), frames);
        Ok(format)
    }

    /// A frame a viewer sent on its channel.
    fn channel(&mut self, viewer: ViewerId, frame: Frame) {
        if !self.viewers.contains(viewer) {
            return;
        }
        let controller = self.viewers.is_controller(viewer);
        match frame.kind {
            // the first input makes its viewer the controller, at its size
            kind::DATA => {
                if !controller {
                    self.take_control(viewer);
                }
                self.input(frame.payload, Input::Person);
            }
            kind::CREDIT => {
                if let Ok(bytes) = <[u8; 4]>::try_from(frame.payload.as_slice()) {
                    let bytes = u32::from_le_bytes(bytes);
                    self.with_paint(|viewers, paint, size| {
                        viewers.credit(viewer, bytes, paint, size)
                    });
                }
            }
            kind::RESIZE => {
                if let Ok(size) = <[u8; 4]>::try_from(frame.payload.as_slice()) {
                    let cols = u16::from_le_bytes([size[0], size[1]]);
                    let rows = u16::from_le_bytes([size[2], size[3]]);
                    if cols > 0 && rows > 0 {
                        self.viewers.set_size(viewer, (cols, rows));
                        if controller {
                            self.resize((cols, rows));
                        }
                    }
                }
            }
            kind::CONTROL => match serde_json::from_slice::<Value>(&frame.payload)
                .ok()
                .as_ref()
                .and_then(Value::as_str)
            {
                Some("take") => self.take_control(viewer),
                Some("release") if controller => {
                    self.viewers.release(viewer);
                    self.set_focus(false);
                }
                _ => {}
            },
            // kept for every viewer, applied while it controls: a viewer
            // sends them as it attaches, before it can take control
            kind::FACTS => {
                if let Ok(facts) = serde_json::from_slice::<Value>(&frame.payload) {
                    if controller {
                        self.screen.set_facts(&display_facts(&facts));
                    }
                    self.viewers.set_facts(viewer, facts);
                }
            }
            kind::FOCUS if controller => {
                self.set_focus(frame.payload.first() == Some(&1));
            }
            // a reply from the viewer's own emulator: written, but never
            // the person's input and never a promotion
            kind::REPLY if controller => self.input(frame.payload, Input::Reply),
            kind::CLOSE => self.detach(viewer),
            _ => {}
        }
    }

    /// Makes a viewer the controller, and the terminal its size.
    fn take_control(&mut self, viewer: ViewerId) {
        if self.viewers.promote(viewer) {
            if let Some(facts) = self.viewers.facts_of(viewer) {
                self.screen.set_facts(&display_facts(facts));
            }
            self.set_focus(false);
            if let Some(size) = self.viewers.size_of(viewer) {
                self.resize(size);
            }
        }
    }

    fn detach(&mut self, viewer: ViewerId) {
        let was_controller = self.viewers.is_controller(viewer);
        if self.viewers.remove(viewer) && was_controller {
            // the PTY keeps its size; nobody is watching it now
            self.set_focus(false);
        }
    }

    /// Resizes the PTY and the screen, which the next output is parsed at,
    /// and tells the viewers.
    fn resize(&mut self, size: (u16, u16)) {
        if size == self.size || self.ended {
            return;
        }
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(master) = &state.master {
                let _ = pty::resize(master, size.0, size.1);
            }
            state.info.cols = size.0;
            state.info.rows = size.1;
        }
        self.screen.resize(size.0, size.1, None);
        self.size = size;
        self.context.records.update(&self.id, |record| {
            record["cols"] = json!(size.0);
            record["rows"] = json!(size.1);
        });
        self.context.events.emit(
            "size",
            json!({"terminal": self.id, "cols": size.0, "rows": size.1}),
        );
        self.with_paint(|viewers, paint, size| viewers.resized(size, paint));
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
            cleared_at: None,
            pasted_at: None,
        });
        self.type_on();
    }

    /// When the message being typed can go on, if one is.
    fn typing_due(&self) -> Option<Instant> {
        let typing = self.typing.as_ref()?;
        let submit = &typing.submit;
        Some(match (typing.cleared_at, typing.pasted_at) {
            // after the keys that clear the input, wait for the output to
            // settle before the paste: Cursor drops a paste that comes with
            // them
            (Some(cleared_at), None) => {
                ((self.last_output.max(cleared_at)) + submit.settle).min(cleared_at + submit.max)
            }
            // wait for a starting program to go quiet, within its first
            // 10 seconds
            (None, None) => {
                let startup_ends = self.started + STARTUP;
                if submit.startup_quiet.is_zero() {
                    Instant::now()
                } else {
                    (self.last_output + submit.startup_quiet).min(startup_ends)
                }
            }
            // after the paste, wait for the output to settle, or `max`
            (_, Some(pasted_at)) => {
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
        if typing.cleared_at.is_none() && !typing.submit.clear.is_empty() {
            let _ = input.send(typing.submit.clear.clone().into_bytes());
            typing.cleared_at = Some(Instant::now());
            return;
        }
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

    /// Takes program output. While a viewer waits for a snapshot that the
    /// screen cannot yet take, the output goes in pieces, and the viewers
    /// get their snapshots after the first piece that makes one possible:
    /// a piece no longer than the continuation leaves the parser either
    /// still inside the long sequence or in one the snapshot can replay.
    fn apply(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while !rest.is_empty() && self.viewers.waiting() && !self.screen.paintable() {
            let (piece, after) = rest.split_at(rest.len().min(PIECE));
            self.apply_piece(piece);
            rest = after;
            if self.screen.paintable() {
                self.with_paint(|viewers, paint, size| {
                    for id in viewers.ids() {
                        viewers.flush(id, paint, size);
                    }
                });
            }
        }
        self.apply_piece(rest);
    }

    fn apply_piece(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.last_output = Instant::now();
        let (mut replies, mut signals) = (Vec::new(), Vec::<Signal>::new());
        // the screen answers the program's queries; the filter keeps the
        // answered ones from the viewers, which would answer them again
        let size = self.size;
        let screen = &mut self.screen;
        let mut left = None;
        let mut feed = |bytes: &[u8]| {
            let before = replies.len();
            // the alternate screen, before the program leaves it
            feed_output(
                screen.as_mut(),
                bytes,
                &mut replies,
                &mut signals,
                |screen| {
                    left = Some(Kept {
                        text: screen.text(TextRange::Screen),
                        paint: screen.first_paint(0),
                        size,
                    });
                },
            );
            replies.len() - before
        };
        self.for_viewers.clear();
        self.filter.push(bytes, &mut self.for_viewers, &mut feed);
        if let Some(kept) = left {
            self.left_alternate = Some((kept, Instant::now()));
        }
        self.viewers
            .output(&self.for_viewers, std::time::Instant::now());
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
                    self.viewers
                        .meta_all(&json!({"type": "title", "text": text}));
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
                // what it waits for or works on, such as "approve Bash: ls"
                if !status.message.is_empty() {
                    params["msg"] = json!(trimmed(&status.message, 240));
                }
                events.emit("program_status", params);
            }
            // a program that asks for focus reports learns at once whether
            // a focused viewer controls the terminal
            Signal::FocusReporting(on) => {
                self.focus_reporting = on;
                if on {
                    self.write_focus();
                }
            }
        }
    }

    /// Writes the hand-over file: a JSON header line with the format, the
    /// size, the last input and the bytes the filter holds, then the
    /// screen, as libghostty-vt's binary snapshot when the next host reads
    /// the same build, else as a first paint any emulator reads.
    fn write_hand_over(&self, file: &Path, formats: &[String]) -> io::Result<()> {
        let binary = self.screen.binary_snapshot();
        let paint = || self.screen.first_paint(crate::server::SCROLLBACK_LINES);
        // the first format the next host reads that this screen produces
        let (format, bytes) = formats
            .iter()
            .find_map(|wanted| {
                if wanted == "vt" {
                    return Some(("vt".to_string(), paint()));
                }
                binary
                    .as_ref()
                    .filter(|b| b.format == *wanted)
                    .map(|b| (b.format.clone(), b.bytes.clone()))
            })
            .unwrap_or_else(|| ("vt".to_string(), paint()));
        let header = json!({
            "format": format,
            "cols": self.size.0,
            "rows": self.size.1,
            "last_input_at": self.state.lock().unwrap_or_else(|e| e.into_inner()).last_input_at,
            "held": self.filter.held(),
        });
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let partial = file.with_extension("partial");
        let mut out = std::fs::File::create(&partial)?;
        out.write_all(header.to_string().as_bytes())?;
        out.write_all(b"\n")?;
        out.write_all(&bytes)?;
        out.sync_all()?;
        std::fs::rename(partial, file)
    }

    /// The screen as it is now, with its history, to keep as the one the
    /// terminal ended with.
    fn keep(&self) -> Kept {
        Kept {
            text: self.text(KEPT_LINES),
            paint: self.screen.first_paint(KEPT_LINES),
            size: self.size,
        }
    }

    /// The screen as a paint, with its size: the one kept once the
    /// terminal has ended with one, otherwise the screen now.
    fn paint(&self) -> (Vec<u8>, (u16, u16)) {
        match &self.kept {
            Some(kept) if self.ended => (kept.paint.clone(), kept.size),
            _ => (self.screen.first_paint(KEPT_LINES), self.size),
        }
    }

    fn text(&self, lines: usize) -> String {
        if self.ended
            && let Some(kept) = &self.kept
        {
            return kept.text.clone();
        }
        let history = self.screen.text(TextRange::History);
        let kept: Vec<&str> = history.lines().collect();
        let mut text: String = kept[kept.len().saturating_sub(lines)..]
            .iter()
            .map(|line| format!("{line}\n"))
            .collect();
        text.push_str(&self.screen.text(TextRange::Screen));
        text
    }

    /// Records the exit, stops the reader and the writer, has the holder
    /// forget the terminal and reap the program, which nothing signals any
    /// more, keeps the final screen and reports the exit.
    fn end(&mut self, status: Exit) {
        // a program that left the alternate screen as it exited ended
        // with what that screen showed
        if self.kept.is_none()
            && let Some((kept, at)) = self.left_alternate.take()
            && at.elapsed() < LEFT_WITHIN
        {
            self.kept = Some(kept);
        }
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
            (reason, ended_at)
        };
        if let Some(forget) = self.forget.take() {
            forget();
        }
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
        let mut last = params.clone();
        last["type"] = json!("exit");
        if let Some(fields) = last.as_object_mut() {
            fields.remove("terminal");
        }
        self.viewers.close_all(&last);
        self.ended = true;
        self.context.events.emit("exited", params);
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .exit_reported = true;
    }
}

/// Display facts from a FACTS frame: `{color_scheme, foreground,
/// background, cursor}`, colours as `#rrggbb`.
fn display_facts(facts: &Value) -> DisplayFacts {
    let rgb = |key: &str| {
        let hex = facts.get(key)?.as_str()?.strip_prefix('#')?;
        let value = u32::from_str_radix(hex, 16)
            .ok()
            .filter(|_| hex.len() == 6)?;
        Some(Rgb {
            r: (value >> 16) as u8,
            g: (value >> 8) as u8,
            b: value as u8,
        })
    };
    DisplayFacts {
        color_scheme: match facts.get("color_scheme").and_then(Value::as_str) {
            Some("dark") => Some(ColorScheme::Dark),
            Some("light") => Some(ColorScheme::Light),
            _ => None,
        },
        foreground: rgb("foreground"),
        background: rgb("background"),
        cursor: rgb("cursor"),
    }
}

/// The output fed at a time while a viewer waits for a snapshot: well
/// under the 64 KiB continuation.
const PIECE: usize = 4 * 1024;

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
            clear: String::new(),
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
        // a marker that a removal would put back together
        assert_eq!(
            paste(&submit("a\x1b[20\x1b[201~1~\rb", true)),
            b"\x1b[200~a\rb\x1b[201~"
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
