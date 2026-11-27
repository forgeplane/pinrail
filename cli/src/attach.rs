//! `pinrail attach`: puts this terminal on a terminal of Pinrail's host,
//! through the core's attach WebSocket. The person's terminal draws the
//! agent's screen, and their keys go to the agent; Ctrl-\ then d detaches,
//! and the agent keeps running.
//!
//! Adapted from herdr-core (Apache-2.0), commit ec63ef3,
//! crates/herdr-core/src/attach.rs and the `console` module of
//! crates/herdr-core/src/platform/unix.rs; changed by Pinrail: it speaks the
//! core's attach WebSocket, detaches on a two-key chord, and polls the
//! socket and the keyboard in one loop. The Windows console is adapted
//! from the `console` module of crates/herdr-core/src/platform/windows.rs
//! at the same commit, changed to wait on the socket too.

use std::io::ErrorKind;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Args;
use serde_json::{Value, json};
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::{Message, WebSocket};

use crate::api::{Client, segment};

#[derive(Args, Debug)]
pub struct AttachArgs {
    /// The run, such as run_01J…, or the session, such as ses_01J…
    target: String,
    /// Lines of the agent's history to draw above its screen
    #[arg(long, default_value_t = 5_000)]
    history: usize,
}

impl AttachArgs {
    /// The arguments for attaching to `run`, with the default history.
    pub fn of(run: String) -> Self {
        AttachArgs {
            target: run,
            history: 5_000,
        }
    }
}

/// Asks the person's terminal for its colours and cell size; the reply to
/// DA1 marks the end of the answers.
const ASK_DISPLAY: &[u8] = b"\x1b]10;?\x1b\\\x1b]11;?\x1b\\\x1b]12;?\x1b\\\x1b[16t\x1b[c";

/// Undoes what the agent may have changed in the person's terminal:
/// synchronized output first, as it freezes the screen, the kitty key flags
/// on both screens, as each keeps its own, then the modes back to their
/// defaults, the scroll region, cursor, character set and colours. Origin
/// mode and the scroll region move the cursor home, so its position is
/// saved around them, and the next prompt follows the agent's last line.
const RESET: &[u8] = b"\x1b[?2026l\x1b[?1049h\x1b[=0;1u\x1b[?1049l\x1b[?1047l\x1b[?47l\x1b[=0;1u\x1b[>4;0m\
\x1b7\x1b[4l\x1b[20l\x1b[?1l\x1b[?5l\x1b[?6l\x1b[?66l\x1b[?67l\x1b[?69l\x1b[?9l\x1b[?1000l\x1b[?1001l\x1b[?1002l\
\x1b[?1003l\x1b[?1004l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l\x1b[?2004l\x1b[?2031l\x1b[?2033l\
\x1b[?2048l\x1b[?7h\x1b[?25h\x1b[?1007h\x1b[r\x1b8\x1b[0 q\x1b(B\x1b[0m\
\x1b]104\x1b\\\x1b]110\x1b\\\x1b]111\x1b\\\x1b]112\x1b\\\x1b]22;\x1b\\\r\n";

/// Ctrl-\ as a control byte, and its key code in the keyboard protocols.
const PREFIX_BYTE: u8 = 0x1c;
const PREFIX_CODE: u32 = 92;
/// The key after the prefix that detaches: d.
const DETACH_BYTE: u8 = b'd';
const DETACH_CODE: u32 = 100;

/// A key in one of the encodings the person's terminal may use: a plain
/// byte, a kitty `CSI code;M[:E]u` (press or repeat, lock keys ignored), or
/// modifyOtherKeys `CSI 27;M;code~`. Returns its length when the key at the
/// start of `input` is `byte` or `code` with exactly the modifiers `mods`
/// (0 for none, 4 for Ctrl, as the protocols count them less one).
fn key_at(input: &[u8], byte: u8, code: u32, mods: u32) -> Option<usize> {
    if input.first() == Some(&byte) {
        return Some(1);
    }
    let rest = input.strip_prefix(b"\x1b[")?;
    let end = rest.iter().position(|b| (0x40..=0x7e).contains(b))?;
    let params = std::str::from_utf8(&rest[..end]).ok()?;
    let fields: Vec<&str> = params.split(';').collect();
    let code = code.to_string();
    // Caps Lock (64) and Num Lock (128) change nothing
    let only = |m: &str| {
        m.parse::<u32>()
            .is_ok_and(|m| m >= 1 && (m - 1) & !(64 | 128) == mods)
    };
    let matched = match (rest[end], fields.as_slice()) {
        (b'u', [key]) => mods == 0 && key.split(':').next() == Some(code.as_str()),
        (b'u', [key, modifiers, ..]) => {
            let (modifiers, event) = modifiers.split_once(':').unwrap_or((modifiers, "1"));
            key.split(':').next() == Some(code.as_str()) && only(modifiers) && event != "3"
        }
        (b'~', ["27", modifiers, key]) => *key == code && only(modifiers),
        _ => false,
    };
    matched.then_some(2 + end + 1)
}

/// What the detach chord does to a piece of input.
#[derive(Debug, PartialEq)]
enum Chord {
    /// No chord: the input goes to the agent as it is.
    None,
    /// The prefix ends the input: wait for the next key.
    Armed,
    /// The chord is complete: send what is before it, then detach.
    Detach,
}

/// Looks for Ctrl-\ then d in `input`, after a prefix the last input ended
/// with when `armed`. A prefix followed by another key is sent on as the
/// control byte, so Ctrl-\ still reaches the agent; the returned input
/// holds that change.
fn chord(input: &[u8], armed: bool) -> (Vec<u8>, Chord) {
    let mut out = Vec::with_capacity(input.len() + 1);
    let mut i = 0;
    let mut waiting = armed;
    while i <= input.len() {
        if waiting {
            if i == input.len() {
                return (out, Chord::Armed);
            }
            if key_at(&input[i..], DETACH_BYTE, DETACH_CODE, 0).is_some() {
                return (out, Chord::Detach);
            }
            out.push(PREFIX_BYTE);
            waiting = false;
            continue;
        }
        if i == input.len() {
            break;
        }
        if let Some(len) = key_at(&input[i..], PREFIX_BYTE, PREFIX_CODE, 4) {
            waiting = true;
            i += len;
            continue;
        }
        out.push(input[i]);
        i += 1;
    }
    (out, Chord::None)
}

/// Where a trailing escape sequence that has not fully arrived starts, or
/// `input.len()` when there is none: reads do not keep sequences whole.
fn incomplete_tail(input: &[u8]) -> usize {
    let mut at = 0;
    while let Some(found) = input[at..].iter().position(|b| *b == 0x1b) {
        let esc = at + found;
        let rest = &input[esc + 1..];
        // how long the sequence after ESC is, if it is all there
        let len = match rest.first() {
            None => None,
            // CSI: parameters, then one final byte
            Some(b'[') => rest[1..]
                .iter()
                .position(|b| (0x40..=0x7e).contains(b))
                .map(|p| p + 2),
            // OSC, DCS, APC: until BEL or ST; a runaway one is let through
            // rather than held forever
            Some(b']' | b'P' | b'_') => (1..rest.len())
                .find_map(|j| match rest[j] {
                    0x07 => Some(j + 1),
                    0x1b if rest.get(j + 1) == Some(&b'\\') => Some(j + 2),
                    _ => None,
                })
                .or((rest.len() > 512).then_some(rest.len())),
            Some(b'O') => (rest.len() >= 2).then_some(2),
            Some(_) => Some(1),
        };
        match len {
            Some(n) => at = esc + 1 + n,
            None => return esc,
        }
    }
    input.len()
}

/// The agent's output as the person's terminal gets it. Focus reports are
/// ours, as they tell the host which viewer the person looks at, so a
/// paint or an agent that turns them off turns them back on, where it
/// turned them off: an output can end inside a character the next
/// finishes.
fn shown(payload: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    const OFF: &[u8] = b"\x1b[?1004l";
    let mut out = std::borrow::Cow::Borrowed(payload);
    while let Some(at) = out.windows(OFF.len()).position(|w| w == OFF) {
        out.to_mut()[at + OFF.len() - 1] = b'h';
    }
    out
}

/// The person's terminal's colours and cell size, as the facts message
/// carries them.
#[derive(Debug, Default, Clone, PartialEq)]
struct Facts {
    fg: Option<String>,
    bg: Option<String>,
    cursor: Option<String>,
    cell: Option<[u32; 2]>,
}

impl Facts {
    fn message(&self) -> Value {
        json!({"t": "facts", "fg": self.fg, "bg": self.bg, "cursor": self.cursor, "cell": self.cell})
    }
}

/// Takes the replies to [`ASK_DISPLAY`] out of the input. Returns the rest
/// and whether the reply to DA1, the last, arrived.
fn take_replies(input: &[u8], facts: &mut Facts) -> (Vec<u8>, bool) {
    let mut rest = Vec::with_capacity(input.len());
    let mut done = false;
    let mut i = 0;
    while i < input.len() {
        let tail = &input[i..];
        let osc = [b"\x1b]10;", b"\x1b]11;", b"\x1b]12;"]
            .iter()
            .position(|p| tail.starts_with(*p));
        if let Some(which) = osc {
            let end = tail
                .windows(2)
                .position(|w| w == b"\x1b\\")
                .map(|p| (p, p + 2))
                .or_else(|| tail.iter().position(|b| *b == 0x07).map(|p| (p, p + 1)));
            if let Some((body_end, len)) = end {
                let color = parse_rgb(&tail[5..body_end]);
                match which {
                    0 => facts.fg = color,
                    1 => facts.bg = color,
                    _ => facts.cursor = color,
                }
                i += len;
                continue;
            }
        }
        if let Some(body) = tail.strip_prefix(b"\x1b[6;")
            && let Some(t) = body.iter().position(|b| *b == b't')
        {
            let numbers: Vec<u32> = std::str::from_utf8(&body[..t])
                .unwrap_or("")
                .split(';')
                .filter_map(|n| n.parse().ok())
                .collect();
            if let [height, width] = numbers[..] {
                facts.cell = Some([width, height]);
            }
            i += 4 + t + 1;
            continue;
        }
        if let Some(body) = tail.strip_prefix(b"\x1b[?")
            && let Some(c) = body.iter().position(|b| !b.is_ascii_digit() && *b != b';')
            && body[c] == b'c'
        {
            done = true;
            i += 3 + c + 1;
            continue;
        }
        rest.push(input[i]);
        i += 1;
    }
    (rest, done)
}

/// `rgb:RRRR/GGGG/BBBB`, of any hex width, as `#rrggbb`.
fn parse_rgb(body: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(body).ok()?.strip_prefix("rgb:")?;
    let parts: Vec<&str> = s.split('/').collect();
    let [r, g, b] = parts[..] else { return None };
    let byte = |p: &str| u8::from_str_radix(p.get(..2)?, 16).ok();
    Some(format!("#{:02x}{:02x}{:02x}", byte(r)?, byte(g)?, byte(b)?))
}

/// The host's terminal of a run, as the core names it, `local:t_…`: of the
/// run, or of the live run in the session `ses_…` or `<agent>/<id>`.
fn terminal_of(client: &Client, target: &str) -> Result<String> {
    let run = if target.starts_with("run_") {
        client.get(&format!("/api/v1/runs/{}", segment(target)), &[])?
    } else if target.starts_with("ses_") || target.contains('/') {
        let runs = client.get(
            "/api/v1/runs",
            &[
                ("state", "live".to_string()),
                ("session", target.to_string()),
            ],
        )?;
        match runs["runs"].as_array().and_then(|r| r.first()) {
            Some(run) => run.clone(),
            None => bail!("no agent Pinrail started runs in session {target}"),
        }
    } else {
        bail!("name a run, such as run_01J…, or a session, such as ses_01J…");
    };
    if run["state"] == "ended" {
        bail!(
            "{} has ended ({})",
            run["id"].as_str().unwrap_or(target),
            run["end_reason"].as_str().unwrap_or("ended")
        );
    }
    run["terminal"]
        .as_str()
        .map(str::to_string)
        .with_context(|| format!("{target} has no terminal yet; try again in a moment"))
}

/// Attaches this terminal, and returns what to print once the person's
/// terminal is restored: why the attach ended.
pub fn run(client: &Client, args: &AttachArgs) -> Result<String> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("pinrail attach needs a terminal: run it in one, not from a pipe or a script");
    }
    let terminal = terminal_of(client, &args.target)?;
    let ticket = client.post(
        &format!("/api/v1/terminals/{}/tickets", segment(&terminal)),
        None,
    )?;
    let ticket = ticket["ticket"].as_str().unwrap_or_default();
    let (cols, rows) = console::window_size();
    let url = format!(
        "{}/api/v1/terminals/{}/attach?ticket={ticket}&cols={cols}&rows={rows}&history={}&formats=vt",
        client.base().replacen("http", "ws", 1),
        segment(&terminal),
        args.history
    );
    let (mut socket, _) =
        tungstenite::connect(url.as_str()).context("connecting to the terminal")?;
    let ended = {
        let _raw = console::RawMode::enter();
        console::catch_termination();
        let ended = relay(&mut socket);
        console::out(RESET);
        ended
    };
    let _ = socket.close(None);
    let _ = socket.flush();
    Ok(match ended? {
        Ended::Exited(exit) => format!(
            "[{} ended: {}]",
            args.target,
            exit["reason"].as_str().unwrap_or("exited")
        ),
        Ended::Detached => format!(
            "[detached from {}; it keeps running: pinrail attach {}]",
            args.target, args.target
        ),
        Ended::Closed(reason) => format!("[{reason}; pinrail attach {} to try again]", args.target),
    })
}

/// Why the attach ended.
enum Ended {
    /// The agent exited, as the exit message says.
    Exited(Value),
    /// The person detached, or closed their terminal.
    Detached,
    /// The core closed the socket, for this reason.
    Closed(String),
}

type Socket = WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>;

fn stream_fd(socket: &Socket) -> console::Fd {
    match socket.get_ref() {
        #[cfg(unix)]
        tungstenite::stream::MaybeTlsStream::Plain(stream) => {
            std::os::fd::AsRawFd::as_raw_fd(stream)
        }
        #[cfg(windows)]
        tungstenite::stream::MaybeTlsStream::Plain(stream) => {
            std::os::windows::io::AsRawSocket::as_raw_socket(stream)
        }
        _ => console::NO_FD,
    }
}

fn send(socket: &mut Socket, message: Message) -> Result<()> {
    match socket.send(message) {
        Ok(()) => Ok(()),
        // queued; the next flush sends it
        Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => Ok(()),
        Err(e) => Err(e).context("sending to the terminal"),
    }
}

fn text(socket: &mut Socket, value: Value) -> Result<()> {
    send(socket, Message::Text(value.to_string().into()))
}

/// Relays between the person's terminal and the socket until either ends.
fn relay(socket: &mut Socket) -> Result<Ended> {
    if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_ref() {
        stream
            .set_nonblocking(true)
            .context("setting up the connection")?;
    }
    let fd = stream_fd(socket);
    // the person's terminal's colours and cell size, for the agent's queries
    console::out(ASK_DISPLAY);
    console::out(b"\x1b[?1004h");
    let mut asking = Some(Instant::now());
    let mut facts = Facts::default();
    let mut sent = Facts::default();
    let mut size = console::window_size();
    let mut exit: Option<Value> = None;
    let mut carry: Vec<u8> = Vec::new();
    let mut armed = false;
    loop {
        // a sequence cut off at the end of a read waits for its rest; nothing
        // more within 50 ms means it was a lone Esc after all
        let wait = if carry.is_empty() { 100 } else { 50 };
        let (keys, socket_ready) = console::wait(fd, wait);

        if socket_ready {
            let mut credit = 0usize;
            loop {
                match socket.read() {
                    Ok(Message::Binary(bytes)) => match bytes.first() {
                        Some(0x01) if bytes.len() >= 9 => {
                            console::out(&shown(&bytes[9..]));
                            credit += bytes.len() - 9;
                        }
                        Some(0x02) => {
                            console::out(&shown(&bytes[1..]));
                            credit += bytes.len() - 1;
                        }
                        _ => {}
                    },
                    Ok(Message::Text(message)) => {
                        let value: Value = serde_json::from_str(&message).unwrap_or_default();
                        if value["t"] == "exit" {
                            exit = Some(value);
                        }
                    }
                    Ok(Message::Close(frame)) => {
                        if let Some(exit) = exit {
                            return Ok(Ended::Exited(exit));
                        }
                        return Ok(Ended::Closed(closed_reason(frame.map(|f| f.code))));
                    }
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(_) => {
                        if let Some(exit) = exit {
                            return Ok(Ended::Exited(exit));
                        }
                        return Ok(Ended::Closed("the connection to Pinrail was lost".into()));
                    }
                }
            }
            if credit > 0 {
                text(socket, json!({"t": "credit", "bytes": credit}))?;
            }
        }

        let mut input = std::mem::take(&mut carry);
        match keys {
            console::Keys::Some(chunk) => {
                input.extend(chunk);
                carry = input.split_off(incomplete_tail(&input));
            }
            console::Keys::None => {}
            // the person's terminal went away
            console::Keys::Closed => return Ok(Ended::Detached),
        }
        if console::terminated() {
            return Ok(Ended::Detached);
        }
        // replies are ours even when they come late
        let (rest, finished) = take_replies(&input, &mut facts);
        input = rest;
        if asking.is_some_and(|since| finished || since.elapsed() > Duration::from_millis(500)) {
            asking = None;
        }
        if asking.is_none() && facts != sent {
            text(socket, facts.message())?;
            sent = facts.clone();
        }
        for (sequence, focused) in [(&b"\x1b[I"[..], true), (&b"\x1b[O"[..], false)] {
            while let Some(at) = input.windows(3).position(|w| w == sequence) {
                input.drain(at..at + 3);
                text(socket, json!({"t": "focus", "in": focused}))?;
                // the theme may have changed while the person was away
                if focused && asking.is_none() {
                    console::out(ASK_DISPLAY);
                    asking = Some(Instant::now());
                }
            }
        }
        let (input, chord) = chord(&input, armed);
        armed = chord == Chord::Armed;
        if !input.is_empty() {
            let mut message = vec![0x10];
            message.extend(&input);
            send(socket, Message::Binary(message.into()))?;
        }
        if chord == Chord::Detach {
            let _ = socket.flush();
            return Ok(Ended::Detached);
        }
        let now = console::window_size();
        if now != size {
            size = now;
            text(socket, json!({"t": "resize", "cols": now.0, "rows": now.1}))?;
        }
        match socket.flush() {
            Ok(()) => {}
            Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => {}
            Err(_) => return Ok(Ended::Closed("the connection to Pinrail was lost".into())),
        }
    }
}

/// What a close code says, as the person reads it.
fn closed_reason(code: Option<CloseCode>) -> String {
    match code.map(u16::from) {
        Some(4001) => "the attach was refused".into(),
        Some(4004) => "the terminal is gone".into(),
        Some(4008) => "this terminal read too slowly".into(),
        Some(4010) => "Pinrail lost its terminal host".into(),
        Some(4011) => "another attach replaced this one".into(),
        _ => "Pinrail closed the connection".into(),
    }
}

/// The person's terminal: raw mode, unbuffered output, and waiting on the
/// keyboard and the socket together.
#[cfg(unix)]
#[allow(unsafe_code)]
mod console {
    use std::fs::File;
    use std::io::Write;
    use std::os::fd::FromRawFd;
    use std::sync::atomic::{AtomicBool, Ordering};

    static TERMINATED: AtomicBool = AtomicBool::new(false);

    /// The socket's descriptor.
    pub type Fd = i32;
    pub const NO_FD: Fd = -1;

    extern "C" fn on_signal(_: libc::c_int) {
        TERMINATED.store(true, Ordering::SeqCst);
    }

    /// A kill (TERM, HUP, INT) only sets a flag: the loop sees it within
    /// one wait and leaves through the normal cleanup, so the terminal is
    /// given back.
    pub fn catch_termination() {
        for signal in [libc::SIGTERM, libc::SIGHUP, libc::SIGINT] {
            // SAFETY: the handler only stores to an atomic, which is
            // async-signal-safe.
            unsafe { libc::signal(signal, on_signal as *const () as libc::sighandler_t) };
        }
    }

    pub fn terminated() -> bool {
        TERMINATED.load(Ordering::SeqCst)
    }

    /// Raw mode on standard input, restored when dropped.
    pub struct RawMode(Option<libc::termios>);

    impl RawMode {
        pub fn enter() -> RawMode {
            // SAFETY: a termios is plain data, for which zero is valid, and
            // tcgetattr fills it.
            let mut saved: libc::termios = unsafe { std::mem::zeroed() };
            // SAFETY: as above.
            if unsafe { libc::tcgetattr(0, &mut saved) } != 0 {
                return RawMode(None);
            }
            let mut raw = saved;
            // SAFETY: both take a valid termios.
            unsafe {
                libc::cfmakeraw(&mut raw);
                libc::tcsetattr(0, libc::TCSANOW, &raw);
            }
            RawMode(Some(saved))
        }
    }

    impl Drop for RawMode {
        fn drop(&mut self) {
            if let Some(saved) = &self.0 {
                // SAFETY: `saved` is the termios tcgetattr filled.
                unsafe { libc::tcsetattr(0, libc::TCSANOW, saved) };
            }
        }
    }

    /// Writes to standard output at once, unbuffered.
    pub fn out(bytes: &[u8]) {
        // SAFETY: descriptor 1 stays open for the process's life, and
        // ManuallyDrop never closes it.
        let mut stdout = std::mem::ManuallyDrop::new(unsafe { File::from_raw_fd(1) });
        let _ = stdout.write_all(bytes);
    }

    pub fn window_size() -> (u16, u16) {
        crate::runs::terminal_size().unwrap_or((80, 24))
    }

    /// What the keyboard gave.
    pub enum Keys {
        Some(Vec<u8>),
        None,
        Closed,
    }

    /// Waits up to `ms` for the keyboard or the socket `fd`. Returns what
    /// the keyboard gave, and whether the socket has something to read.
    pub fn wait(fd: Fd, ms: i32) -> (Keys, bool) {
        let mut fds = [
            libc::pollfd {
                fd: 0,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: `fds` holds two pollfds, as the count says.
        if unsafe { libc::poll(fds.as_mut_ptr(), 2, ms) } <= 0 {
            // a timeout, or a signal: the socket may still hold data the
            // WebSocket has buffered
            return (Keys::None, true);
        }
        let socket = fds[1].revents != 0;
        if fds[0].revents == 0 {
            return (Keys::None, socket);
        }
        let mut buf = [0u8; 8192];
        // SAFETY: the buffer is valid for its length.
        let n = unsafe { libc::read(0, buf.as_mut_ptr().cast(), buf.len()) };
        let keys = if n > 0 {
            Keys::Some(buf[..n as usize].to_vec())
        } else {
            Keys::Closed
        };
        (keys, socket)
    }
}

/// The person's console on Windows: keys as VT bytes in and VT sequences
/// out, nothing cooked, and waiting on the keyboard and the socket
/// together.
#[cfg(windows)]
#[allow(unsafe_code)]
mod console {
    use std::sync::atomic::{AtomicBool, Ordering};

    use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
    use windows_sys::Win32::Networking::WinSock::{
        FD_CLOSE, FD_READ, WSACreateEvent, WSAEventSelect, WSAResetEvent,
    };
    use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
    use windows_sys::Win32::System::Console::{
        DISABLE_NEWLINE_AUTO_RETURN, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
        ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleCP,
        GetConsoleMode, GetConsoleOutputCP, GetStdHandle, INPUT_RECORD, KEY_EVENT,
        PeekConsoleInputW, ReadConsoleInputW, STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        SetConsoleCP, SetConsoleCtrlHandler, SetConsoleMode, SetConsoleOutputCP,
    };
    use windows_sys::Win32::System::Threading::WaitForMultipleObjects;
    use windows_sys::core::BOOL;

    static TERMINATED: AtomicBool = AtomicBool::new(false);

    /// The socket.
    pub type Fd = u64;
    pub const NO_FD: Fd = u64::MAX;

    unsafe extern "system" fn on_ctrl(_: u32) -> BOOL {
        TERMINATED.store(true, Ordering::SeqCst);
        1
    }

    /// Ctrl+Break, closing the window or signing out only set a flag: the
    /// loop sees it within one wait and leaves through the normal cleanup,
    /// so the console is given back. Ctrl+C, in raw mode, is a key the agent
    /// gets.
    pub fn catch_termination() {
        // SAFETY: the handler only stores to an atomic.
        unsafe { SetConsoleCtrlHandler(Some(on_ctrl), 1) };
    }

    pub fn terminated() -> bool {
        TERMINATED.load(Ordering::SeqCst)
    }

    fn std(which: STD_HANDLE) -> HANDLE {
        // SAFETY: the process's own standard handles, which it does not close.
        unsafe { GetStdHandle(which) }
    }

    /// The console's modes and code pages, restored when dropped.
    pub struct RawMode(Option<(u32, u32, u32, u32)>);

    impl RawMode {
        pub fn enter() -> RawMode {
            // SAFETY: each call takes a standard handle and plain integers.
            unsafe {
                let (mut input, mut output) = (0u32, 0u32);
                if GetConsoleMode(std(STD_INPUT_HANDLE), &mut input) == 0
                    || GetConsoleMode(std(STD_OUTPUT_HANDLE), &mut output) == 0
                {
                    return RawMode(None);
                }
                let (cp_out, cp_in) = (GetConsoleOutputCP(), GetConsoleCP());
                let raw = (input
                    & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT))
                    | ENABLE_VIRTUAL_TERMINAL_INPUT;
                SetConsoleMode(std(STD_INPUT_HANDLE), raw);
                SetConsoleMode(
                    std(STD_OUTPUT_HANDLE),
                    output | ENABLE_VIRTUAL_TERMINAL_PROCESSING | DISABLE_NEWLINE_AUTO_RETURN,
                );
                // keys are read, and output written, as UTF-8 bytes
                SetConsoleOutputCP(65001);
                SetConsoleCP(65001);
                RawMode(Some((input, output, cp_out, cp_in)))
            }
        }
    }

    impl Drop for RawMode {
        fn drop(&mut self) {
            if let Some((input, output, cp_out, cp_in)) = self.0 {
                // SAFETY: as in `enter`.
                unsafe {
                    SetConsoleMode(std(STD_INPUT_HANDLE), input);
                    SetConsoleMode(std(STD_OUTPUT_HANDLE), output);
                    SetConsoleOutputCP(cp_out);
                    SetConsoleCP(cp_in);
                }
            }
        }
    }

    /// Writes to standard output at once, as bytes: the console takes them
    /// as UTF-8, a character cut between two writes included.
    pub fn out(mut bytes: &[u8]) {
        while !bytes.is_empty() {
            let mut written = 0u32;
            let len = bytes.len().min(u32::MAX as usize) as u32;
            // SAFETY: the buffer is valid for `len` bytes.
            let ok = unsafe {
                WriteFile(
                    std(STD_OUTPUT_HANDLE),
                    bytes.as_ptr(),
                    len,
                    &mut written,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 || written == 0 {
                return;
            }
            bytes = &bytes[written as usize..];
        }
    }

    pub fn window_size() -> (u16, u16) {
        crate::runs::terminal_size().unwrap_or((80, 24))
    }

    /// What the keyboard gave.
    pub enum Keys {
        Some(Vec<u8>),
        None,
        Closed,
    }

    /// The event the socket sets when it has something to read, made once.
    fn socket_event(socket: Fd) -> Option<HANDLE> {
        static EVENT: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
        let event = *EVENT.get_or_init(|| {
            if socket == NO_FD {
                return None;
            }
            // SAFETY: a new event, kept for the life of the process, which
            // the socket sets; selecting events keeps the socket
            // non-blocking, as the relay has it.
            unsafe {
                let event = WSACreateEvent();
                if event == 0 {
                    return None;
                }
                (WSAEventSelect(socket as usize, event, (FD_READ | FD_CLOSE) as i32) == 0)
                    .then_some(event as usize)
            }
        });
        event.map(|e| e as HANDLE)
    }

    /// Waits up to `ms` for the keyboard or the socket. Returns what the
    /// keyboard gave, and whether the socket may have something to read.
    /// Console events that are not keys, such as focus or a resize, wake the
    /// wait without bytes: they are taken off and skipped.
    pub fn wait(socket: Fd, ms: i32) -> (Keys, bool) {
        let input = std(STD_INPUT_HANDLE);
        let Some(event) = socket_event(socket) else {
            return (Keys::None, true);
        };
        // SAFETY: both handles are open for the wait.
        let woke =
            unsafe { WaitForMultipleObjects(2, [input, event].as_ptr(), 0, ms.max(0) as u32) };
        if woke == WAIT_OBJECT_0 + 1 {
            // reset before reading, so what arrives meanwhile sets it again
            // SAFETY: the event is open.
            unsafe { WSAResetEvent(event as isize) };
            return (Keys::None, true);
        }
        if woke != WAIT_OBJECT_0 {
            // a timeout: the WebSocket may hold data it has buffered
            return (Keys::None, true);
        }
        (read_keys(input), true)
    }

    fn read_keys(input: HANDLE) -> Keys {
        // SAFETY: the records are plain data the calls fill within their
        // count; the buffer is valid for its length.
        unsafe {
            let mut events: [INPUT_RECORD; 32] = std::mem::zeroed();
            let mut n = 0u32;
            if PeekConsoleInputW(input, events.as_mut_ptr(), events.len() as u32, &mut n) == 0 {
                return Keys::Closed;
            }
            // Shift, Ctrl, Alt, Windows or a lock key alone types nothing,
            // and ReadFile would wait for a key that does
            let modifier = |vk: u16| matches!(vk, 0x10..=0x12 | 0x14 | 0x5B | 0x5C | 0x90 | 0x91 | 0xA0..=0xA5);
            let key = events[..n as usize].iter().any(|e| {
                e.EventType == KEY_EVENT as u16
                    && e.Event.KeyEvent.bKeyDown != 0
                    && !modifier(e.Event.KeyEvent.wVirtualKeyCode)
            });
            if !key {
                ReadConsoleInputW(input, events.as_mut_ptr(), n, &mut n);
                return Keys::None;
            }
            let mut buf = [0u8; 8192];
            let mut got = 0u32;
            if ReadFile(
                input,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut got,
                std::ptr::null_mut(),
            ) == 0
                || got == 0
            {
                return Keys::Closed;
            }
            Keys::Some(buf[..got as usize].to_vec())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sequence_cut_off_at_the_end_of_a_read_is_held_back() {
        assert_eq!(incomplete_tail(b"ab\x1b[92;"), 2, "a key half there");
        assert_eq!(
            incomplete_tail(b"ab\x1b]11;rgb:ff"),
            2,
            "a colour reply half there"
        );
        assert_eq!(
            incomplete_tail(b"ab\x1b]11;rgb:ff\x1b"),
            2,
            "cut inside its terminator"
        );
        assert_eq!(incomplete_tail(b"\x1b[Aab\x1b[1;5"), 5, "after a whole one");
        assert_eq!(incomplete_tail(b"ab\x1b"), 2, "maybe a lone Esc");
        assert_eq!(incomplete_tail(b"ab\x1b[A"), 5);
        assert_eq!(incomplete_tail(b"\x1b]11;rgb:ff\x1b\\x"), 14, "ended by ST");
        assert_eq!(incomplete_tail(b"\x1bx"), 2, "alt+x");
    }

    #[test]
    fn ctrl_backslash_then_d_detaches_in_every_keyboard_encoding() {
        for prefix in [
            &b"\x1c"[..],
            b"\x1b[92;5u",
            b"\x1b[92;5:1u",
            b"\x1b[92;69u",
            b"\x1b[92;133:2u",
            b"\x1b[27;5;92~",
        ] {
            for d in [&b"d"[..], b"\x1b[100u", b"\x1b[100;1u"] {
                let mut input = b"ab".to_vec();
                input.extend(prefix);
                input.extend(d);
                input.extend(b"zz");
                assert_eq!(
                    chord(&input, false),
                    (b"ab".to_vec(), Chord::Detach),
                    "{:?}",
                    String::from_utf8_lossy(&input)
                );
            }
        }
        // neither a release, another modifier, another key, nor a backslash
        for input in [
            &b"\x1b[92;5:3u"[..],
            b"\x1b[92;7u",
            b"\x1b[93;5u",
            b"\x1b[92u",
            b"\\",
        ] {
            assert_eq!(chord(input, false), (input.to_vec(), Chord::None));
        }
    }

    #[test]
    fn the_chord_may_span_two_reads() {
        assert_eq!(chord(b"ab\x1c", false), (b"ab".to_vec(), Chord::Armed));
        assert_eq!(chord(b"d", true), (vec![], Chord::Detach));
    }

    #[test]
    fn ctrl_backslash_followed_by_another_key_reaches_the_agent() {
        assert_eq!(chord(b"\x1cx", false), (b"\x1cx".to_vec(), Chord::None));
        assert_eq!(chord(b"x", true), (b"\x1cx".to_vec(), Chord::None));
        // pressed twice, it is sent once, and the second waits for its key
        assert_eq!(chord(b"\x1c\x1c", false), (b"\x1c".to_vec(), Chord::Armed));
    }

    #[test]
    fn focus_reports_stay_on_whatever_the_agent_or_a_paint_says() {
        assert_eq!(&*shown(b"hi\x1b[?1004l there"), b"hi\x1b[?1004h there");
        assert_eq!(&*shown(b"plain"), b"plain");
    }

    #[test]
    fn display_replies_are_taken_out_of_typed_input() {
        let mut facts = Facts::default();
        let input =
            b"a\x1b]11;rgb:ffff/8080/0000\x1b\\b\x1b]10;rgb:11/22/33\x07\x1b[6;18;9t\x1b[?62;22cc";
        let (rest, done) = take_replies(input, &mut facts);
        assert_eq!(rest, b"abc");
        assert!(done);
        assert_eq!(facts.bg.as_deref(), Some("#ff8000"));
        assert_eq!(facts.fg.as_deref(), Some("#112233"));
        assert_eq!(facts.cell, Some([9, 18]));
        let message = facts.message();
        assert_eq!(message["t"], "facts");
        assert_eq!(message["bg"], "#ff8000");
    }
}
