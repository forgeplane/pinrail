//! One terminal: a reader thread that reads the PTY, an actor task that
//! owns the screen and applies everything in one order, a writer thread
//! that writes to the PTY, and a waiter thread that sees the program exit.

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::OwnedFd;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pinrail_screen::{Options, Screen, Signal, TextRange};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use crate::pty::{self, Exit, Ready, Spawned};

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

/// A running or ended terminal. Dropping it ends its actor.
pub struct Terminal {
    info: Arc<Mutex<Info>>,
    commands: mpsc::UnboundedSender<Command>,
}

impl Terminal {
    /// Starts the threads and the actor of a spawned program, whose screen
    /// is `screen`. Must be called inside the runtime.
    pub fn start(
        id: String,
        spawned: Spawned,
        labels: Value,
        cols: u16,
        rows: u16,
        screen: Box<dyn Screen>,
    ) -> io::Result<Self> {
        let info = Arc::new(Mutex::new(Info {
            id,
            pid: spawned.pid,
            labels,
            cols,
            rows,
            started_at: crate::now(),
            ended_at: None,
            exit: None,
        }));
        let (wake_read, wake_write) = io::pipe()?;
        let (output_tx, output_rx) = mpsc::channel(QUEUED_CHUNKS);
        let (commands, commands_rx) = mpsc::unbounded_channel();
        let (input_tx, input_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let reader_master = File::from(spawned.master.try_clone()?);
        let writer_master = File::from(spawned.master.try_clone()?);
        std::thread::Builder::new()
            .name("pty-reader".to_string())
            .spawn(move || read_pty(reader_master, wake_read, output_tx))?;
        std::thread::Builder::new()
            .name("pty-writer".to_string())
            .spawn(move || write_pty(writer_master, input_rx))?;
        let pid = spawned.pid;
        let exited = commands.clone();
        std::thread::Builder::new()
            .name("pty-waiter".to_string())
            .spawn(move || {
                let exit = pty::wait_exit(pid).unwrap_or(Exit::Code(-1));
                let _ = exited.send(Command::Exited(exit));
            })?;
        tokio::spawn(actor(Actor {
            screen,
            info: info.clone(),
            input: Some(input_tx),
            wake: Some(wake_write),
            master: Some(spawned.master),
            pid,
            output: output_rx,
            commands: commands_rx,
        }));
        Ok(Self { info, commands })
    }

    pub fn info(&self) -> Info {
        self.info.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn live(&self) -> bool {
        self.info().ended_at.is_none()
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

struct Actor {
    screen: Box<dyn Screen>,
    info: Arc<Mutex<Info>>,
    /// To the writer thread; dropped when the terminal ends.
    input: Option<std::sync::mpsc::Sender<Vec<u8>>>,
    /// Written to stop the reader; dropped when the terminal ends.
    wake: Option<io::PipeWriter>,
    /// Kept open until the terminal ends.
    master: Option<OwnedFd>,
    pid: u32,
    output: mpsc::Receiver<Output>,
    commands: mpsc::UnboundedReceiver<Command>,
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

impl Actor {
    fn apply(&mut self, bytes: &[u8]) {
        let (mut replies, mut signals) = (Vec::new(), Vec::<Signal>::new());
        self.screen.feed(bytes, &mut replies, &mut signals);
        if !replies.is_empty()
            && let Some(input) = &self.input
        {
            let _ = input.send(replies);
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

    /// Records the exit, stops the reader and the writer, and reaps the
    /// program, which nothing signals any more.
    fn end(&mut self, status: Exit) {
        {
            let mut info = self.info.lock().unwrap_or_else(|e| e.into_inner());
            info.ended_at = Some(crate::now());
            info.exit = Some(status);
        }
        if let Some(mut wake) = self.wake.take() {
            let _ = wake.write_all(b"x");
        }
        self.input = None;
        self.master = None;
        pty::reap(self.pid);
    }
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
