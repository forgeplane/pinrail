//! `mise run bench:host`: the terminal host's performance budgets.
//!
//! - Parse throughput of one terminal with no viewer, through the query
//!   filter and the screen, on the `tui` and `plain` workloads: at least
//!   250 and 500 MB/s.
//! - Memory of a terminal holding 10,000 lines of 120 columns: at most
//!   15 MiB.
//! - CPU of the host with 10 idle agents: at most 1 % of one core.
//! - First paint of 5,000 lines: at most 10 ms.
//!
//! These have no budget, and are compared with the baseline only:
//!
//! - Output to viewers, through the PTY, the host and its frames, as a
//!   viewer that returns credit reads it: one viewer, and three of the same
//!   terminal.
//! - Attach with the binary snapshot of 10,000 lines: from the request to
//!   the snapshot's end.
//! - First paint of 5,000 lines with a full-screen program on the
//!   alternate screen.
//! - The host's resident memory with 10 terminals of 10,000 lines each.
//!
//! It prints the results as JSON and exits with 1 when a budget fails.
//! With `--baseline <file>`, it also fails when a result is more than 15 %
//! worse than the baseline's, beyond a small margin for the measures whose
//! values are too small for a ratio to mean much, and `--write-baseline
//! <file>` writes this run's results as the baseline. `--skip-budgets`
//! checks the baseline only, for machines other than the reference ones,
//! such as CI's. Each result is the best of three runs.
//!
//!     cargo bench -p pinrail-host --bench host -- [--baseline <file>] [--write-baseline <file>] [--skip-budgets]

use std::time::{Duration, Instant};

use pinrail_host::filter::QueryFilter;
use pinrail_host::terminal::{feed_output, screen};
use serde_json::{Value, json};

const COLS: u16 = 120;
const ROWS: u16 = 40;
const SCROLLBACK: usize = 10_000;
/// The size of each workload, fed in reads of 16 KiB as from a PTY.
const WORKLOAD_BYTES: usize = 32 << 20;
const CHUNK: usize = 16 * 1024;
/// How much worse than the baseline a result may be.
const TOLERANCE: f64 = 0.15;

/// A measure: its name, its unit, the budget, whether more is better, and
/// the margin, in its unit, that a result may be worse than the baseline's
/// on top of the 15 %.
struct Measure {
    name: &'static str,
    unit: &'static str,
    /// None for a measure compared with the baseline only.
    budget: Option<f64>,
    higher_is_better: bool,
    margin: f64,
}

const MEASURES: &[Measure] = &[
    Measure {
        name: "throughput_tui",
        unit: "MB/s",
        budget: Some(250.0),
        higher_is_better: true,
        margin: 0.0,
    },
    Measure {
        name: "throughput_plain",
        unit: "MB/s",
        budget: Some(500.0),
        higher_is_better: true,
        margin: 0.0,
    },
    Measure {
        name: "memory_10k_lines",
        unit: "MiB",
        budget: Some(15.0),
        higher_is_better: false,
        margin: 0.0,
    },
    Measure {
        name: "idle_cpu_10_agents",
        unit: "% of a core",
        budget: Some(1.0),
        higher_is_better: false,
        margin: 0.5,
    },
    Measure {
        name: "first_paint_5k_lines",
        unit: "ms",
        budget: Some(10.0),
        higher_is_better: false,
        margin: 1.0,
    },
    Measure {
        name: "viewer_throughput_tui",
        unit: "MB/s",
        budget: None,
        higher_is_better: true,
        margin: 0.0,
    },
    Measure {
        name: "viewer_throughput_tui_3_viewers",
        unit: "MB/s",
        budget: None,
        higher_is_better: true,
        margin: 0.0,
    },
    Measure {
        name: "attach_binary_10k_lines",
        unit: "ms",
        budget: None,
        higher_is_better: false,
        margin: 5.0,
    },
    Measure {
        name: "first_paint_alt_screen_5k_lines",
        unit: "ms",
        budget: None,
        higher_is_better: false,
        margin: 1.0,
    },
    Measure {
        name: "host_memory_10_terminals",
        unit: "MiB",
        budget: None,
        higher_is_better: false,
        // resident memory includes what the allocator keeps, which varies
        // by tens of MiB from run to run
        margin: 40.0,
    },
];

/// A deterministic random number generator, so that each run feeds the
/// same bytes.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const WORDS: &[&str] = &[
    "fn",
    "let",
    "match",
    "self",
    "return",
    "impl",
    "struct",
    "async",
    "await",
    "terminal",
    "agent",
    "review",
    "src/main.rs",
    "Ok(())",
    "error:",
    "warning",
    "->",
    "=>",
    "{",
    "}",
    "Vec<u8>",
    "pub",
    "mod",
    "use",
];
const UNICODE: &[&str] = &[
    "漢字",
    "日本語",
    "🚀",
    "👩\u{200d}💻",
    "✍\u{fe0f}",
    "│",
    "─",
    "╭─╮",
    "✻",
    "é",
    "→",
    "⏺",
];

/// An ASCII line of 60 to 110 characters.
fn plain_line(r: &mut Rng, out: &mut String) {
    let n = 60 + r.below(50) as usize;
    let mut width = 0;
    while width < n {
        let word = WORDS[r.below(WORDS.len() as u64) as usize];
        out.push_str(word);
        out.push(' ');
        width += word.len() + 1;
    }
    out.push_str("\r\n");
}

/// A line with truecolor on every word, some bold, CJK, emoji and box
/// drawing.
fn styled_line(r: &mut Rng, out: &mut String) {
    for _ in 0..8 + r.below(8) {
        let (red, green, blue) = (r.below(256), r.below(256), r.below(256));
        out.push_str(&format!("\x1b[38;2;{red};{green};{blue}m"));
        if r.below(4) == 0 {
            out.push_str("\x1b[1m");
        }
        if r.below(6) == 0 {
            out.push_str(UNICODE[r.below(UNICODE.len() as u64) as usize]);
        } else {
            out.push_str(WORDS[r.below(WORDS.len() as u64) as usize]);
        }
        out.push_str("\x1b[0m ");
    }
    out.push_str("\r\n");
}

/// Output like Claude Code's: synchronized frames that redraw a 10-line
/// footer in place, and every eighth frame commits 20 styled lines above
/// it.
fn tui(r: &mut Rng) -> String {
    let footer = 10;
    let mut out = "\r\n".repeat(footer);
    let mut frame = 0u64;
    while out.len() < WORKLOAD_BYTES {
        out.push_str("\x1b[?2026h");
        out.push_str(&format!("\x1b[{footer}A\r\x1b[J"));
        if frame.is_multiple_of(8) {
            for _ in 0..20 {
                styled_line(r, &mut out);
            }
        }
        for row in 0..footer {
            out.push_str("\x1b[2K");
            match row {
                0 => out.push_str(&format!(
                    "\x1b[38;5;{}m✻\x1b[0m Thinking… ({}s · esc to interrupt)",
                    160 + frame % 6,
                    frame / 10
                )),
                1 => out.push_str("\x1b[2m╭──────────────────────────────────────────────────────────────╮\x1b[0m"),
                8 => out.push_str("\x1b[2m╰──────────────────────────────────────────────────────────────╯\x1b[0m"),
                _ => {
                    out.push_str("\x1b[2m│\x1b[0m > ");
                    let mut line = String::new();
                    plain_line(r, &mut line);
                    out.push_str(line.trim_end());
                }
            }
            out.push_str("\r\n");
        }
        out.push_str("\x1b[?2026l");
        frame += 1;
    }
    out
}

fn plain(r: &mut Rng) -> String {
    let mut out = String::new();
    while out.len() < WORKLOAD_BYTES {
        plain_line(r, &mut out);
    }
    out
}

/// Feeds `bytes` to a new terminal as the host does with program output:
/// through the query filter, which passes every byte to the screen, and
/// `feed_output`, which keeps the alternate screen a program leaves.
/// Returns the seconds it took.
fn feed(bytes: &[u8]) -> f64 {
    let mut screen = screen(COLS, ROWS, SCROLLBACK).expect("a terminal");
    let mut filter = QueryFilter::default();
    let (mut replies, mut signals, mut for_viewers) = (Vec::new(), Vec::new(), Vec::new());
    let started = Instant::now();
    for chunk in bytes.chunks(CHUNK) {
        for_viewers.clear();
        let mut to_screen = |part: &[u8]| {
            let before = replies.len();
            feed_output(screen.as_mut(), part, &mut replies, &mut signals, |_| {});
            replies.len() - before
        };
        filter.push(chunk, &mut for_viewers, &mut to_screen);
        replies.clear();
        signals.clear();
    }
    started.elapsed().as_secs_f64()
}

fn best_of_three(mut run: impl FnMut() -> f64, higher_is_better: bool) -> f64 {
    let runs = [run(), run(), run()];
    let pick = if higher_is_better { f64::max } else { f64::min };
    runs.into_iter().reduce(pick).unwrap()
}

fn throughput(workload: &str) -> f64 {
    let bytes = workload.as_bytes();
    best_of_three(|| bytes.len() as f64 / feed(bytes) / 1e6, true)
}

/// A terminal that holds `lines` lines of 120 columns, with a colour change
/// on every word.
fn filled(lines: usize) -> Box<dyn pinrail_screen::Screen> {
    let mut screen = screen(COLS, ROWS, SCROLLBACK).expect("a terminal");
    let mut r = Rng(7);
    let (mut replies, mut signals) = (Vec::new(), Vec::new());
    for _ in 0..lines + usize::from(ROWS) {
        let mut line = String::new();
        let mut width = 0;
        while width < usize::from(COLS) - 12 {
            let word = WORDS[r.below(WORDS.len() as u64) as usize];
            line.push_str(&format!("\x1b[38;5;{}m{word} ", r.below(256)));
            width += word.len() + 1;
        }
        line.push_str(&" ".repeat(usize::from(COLS) - width));
        line.push_str("\x1b[0m\r\n");
        screen.feed(line.as_bytes(), &mut replies, &mut signals);
    }
    screen
}

fn memory() -> f64 {
    let screen = filled(10_000);
    screen.memory_bytes() as f64 / (1024.0 * 1024.0)
}

fn first_paint() -> f64 {
    let screen = filled(5_000);
    best_of_three(
        || {
            let started = Instant::now();
            let paint = screen.first_paint(5_000);
            std::hint::black_box(paint);
            started.elapsed().as_secs_f64() * 1_000.0
        },
        false,
    )
}

/// The CPU this process used so far, in seconds.
#[allow(unsafe_code)]
fn cpu_seconds() -> f64 {
    // SAFETY: an rusage is plain integers, for which zero is valid, and
    // getrusage fills the struct it is given.
    let usage = unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut usage);
        usage
    };
    let seconds = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    seconds(usage.ru_utime) + seconds(usage.ru_stime)
}

/// A host serving in this process, its programs through a holder on a
/// thread, so that its CPU and memory are the process's. The folder holds
/// its data until it is dropped.
struct Host {
    _dir: tempfile::TempDir,
    data_dir: std::path::PathBuf,
    registration: pinrail_host::registration::Registration,
}

fn start_host() -> Host {
    use pinrail_host::registration::Registration;
    let dir = tempfile::Builder::new()
        .prefix("ph")
        .tempdir_in("/tmp")
        .expect("a folder");
    let data_dir = dir.path().to_path_buf();
    let config = pinrail_host::server::Config {
        data_dir: data_dir.clone(),
        idle_exit: Duration::from_secs(600),
        ping_every: Duration::from_secs(15),
        // the programs are this process's own, through a holder on a thread
        holder: pinrail_host::holders::Start::Thread,
    };
    std::thread::spawn(move || pinrail_host::server::serve(config));
    let deadline = Instant::now() + Duration::from_secs(10);
    let registration = loop {
        if let Some(registration) = Registration::read(&data_dir.join("host/host.json")) {
            break registration;
        }
        assert!(Instant::now() < deadline, "the host did not start");
        std::thread::sleep(Duration::from_millis(20));
    };
    Host {
        _dir: dir,
        data_dir,
        registration,
    }
}

/// The host's CPU over 5 seconds while it runs 10 terminals whose programs
/// wait. The host runs in this process, so its CPU is the process's.
fn idle_cpu() -> f64 {
    use pinrail_host::client::Client;
    let host = start_host();
    let mut client = Client::connect(&host.registration).expect("a connection to the host");
    let mut terminals = Vec::new();
    for _ in 0..10 {
        let spawned = client
            .call(
                "terminal.spawn",
                json!({"argv": ["sleep", "600"], "cwd": "/", "cols": COLS, "rows": ROWS,
                       "env_base": "host"}),
            )
            .expect("a terminal");
        terminals.push(spawned["terminal"].as_str().unwrap_or_default().to_string());
    }
    // let the terminals settle
    std::thread::sleep(Duration::from_secs(1));
    let result = best_of_three(
        || {
            let (cpu, wall) = (cpu_seconds(), Instant::now());
            std::thread::sleep(Duration::from_secs(5));
            (cpu_seconds() - cpu) / wall.elapsed().as_secs_f64() * 100.0
        },
        false,
    );
    for terminal in terminals {
        let _ = client.call(
            "terminal.stop",
            json!({"terminal": terminal, "grace_ms": 0}),
        );
    }
    result
}

/// The host's one connection, spoken as the core speaks it: calls are
/// answered on channel 0, each attach is a channel of its own, and each
/// DATA frame read is credited back at once. A second connection would
/// replace the first.
struct Connection {
    stream: std::os::unix::net::UnixStream,
    next_id: u64,
}

impl Connection {
    fn open(host: &Host) -> Self {
        let stream = std::os::unix::net::UnixStream::connect(&host.registration.socket)
            .expect("a connection to the host");
        let mut connection = Self { stream, next_id: 0 };
        connection.call("hello", json!({"secret": host.registration.secret}));
        connection
    }

    fn send(&mut self, frame: &pinrail_host::frame::Frame) {
        pinrail_host::frame::write_blocking(&mut self.stream, frame).expect("a frame sent");
    }

    /// Reads the next frame, crediting it when it is DATA on a terminal
    /// channel.
    fn read(&mut self) -> pinrail_host::frame::Frame {
        use pinrail_host::frame::{self, Frame, kind};
        let frame = frame::read_blocking(&mut self.stream)
            .expect("a frame")
            .expect("an open connection");
        if frame.kind == kind::DATA && frame.channel != 0 {
            self.send(&Frame {
                channel: frame.channel,
                kind: kind::CREDIT,
                payload: (frame.payload.len() as u32).to_le_bytes().to_vec(),
            });
        }
        frame
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.send(&pinrail_host::frame::Frame::json(&message));
        loop {
            let frame = self.read();
            if frame.channel == 0
                && let Ok(reply) = serde_json::from_slice::<Value>(&frame.payload)
                && reply["id"] == id
            {
                return reply;
            }
        }
    }

    /// Attaches to `terminal` with `formats`, and answers the channel.
    fn attach(&mut self, terminal: &str, history: u32, formats: &[&str]) -> u32 {
        let reply = self.call(
            "terminal.attach",
            json!({"terminal": terminal, "mode": "observe", "cols": COLS, "rows": ROWS,
                   "history": history, "formats": formats}),
        );
        reply["result"]["channel"].as_u64().expect("a channel") as u32
    }

    /// Detaches the viewer of `channel`.
    fn detach(&mut self, channel: u32) {
        self.send(&pinrail_host::frame::Frame {
            channel,
            kind: pinrail_host::frame::kind::CLOSE,
            payload: Vec::new(),
        });
    }

    fn spawn(&mut self, host: &Host, script: &str) -> String {
        let reply = self.call(
            "terminal.spawn",
            json!({"argv": ["sh", "-c", script], "cwd": host.data_dir, "cols": COLS, "rows": ROWS,
                   "env_base": "host"}),
        );
        reply["result"]["terminal"]
            .as_str()
            .expect("a terminal")
            .to_string()
    }

    /// Waits until the terminal's screen shows `text`.
    fn wait_for(&mut self, terminal: &str, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let reply = self.call("terminal.text", json!({"terminal": terminal, "lines": 0}));
            if reply["result"]["text"]
                .as_str()
                .unwrap_or("")
                .contains(text)
            {
                return;
            }
            assert!(Instant::now() < deadline, "no {text}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn stop(&mut self, terminals: &[&str]) {
        for terminal in terminals {
            self.call(
                "terminal.stop",
                json!({"terminal": terminal, "grace_ms": 0}),
            );
        }
    }
}

/// How many throughput runs started, for each run's own go file.
static RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The marker a program prints after its output, found in the DATA a
/// viewer reads.
const END: &[u8] = b"END-OF-BENCH-OUTPUT";

/// MB/s of `bytes`, written by `cat` to a terminal, as `viewers` viewers
/// read it, each a channel of the connection: from the start until each
/// has read the marker after it.
fn viewer_throughput(bytes: &[u8], viewers: usize) -> f64 {
    let host = start_host();
    let file = host.data_dir.join("output.bin");
    std::fs::write(&file, bytes).expect("the output file");
    let mut connection = Connection::open(&host);
    best_of_three(
        || {
            // waits for a go, so that every viewer is attached first
            let run = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let go = host.data_dir.join(format!("go-{run}"));
            let script = format!(
                "while [ ! -e '{go}' ]; do sleep 0.01; done; cat '{file}'; printf '%s\\n' '{end}'; sleep 600",
                go = go.display(),
                file = file.display(),
                end = String::from_utf8_lossy(END),
            );
            let terminal = connection.spawn(&host, &script);
            let mut tails: std::collections::BTreeMap<u32, Vec<u8>> = (0..viewers)
                .map(|_| (connection.attach(&terminal, 0, &["vt"]), Vec::new()))
                .collect();
            let channels: Vec<u32> = tails.keys().copied().collect();
            let started = Instant::now();
            std::fs::write(&go, b"").expect("the go file");
            while !tails.is_empty() {
                let frame = connection.read();
                if frame.kind != pinrail_host::frame::kind::DATA {
                    continue;
                }
                let Some(tail) = tails.get_mut(&frame.channel) else {
                    continue;
                };
                tail.extend_from_slice(&frame.payload);
                if tail.windows(END.len()).any(|w| w == END) {
                    tails.remove(&frame.channel);
                } else {
                    let keep = tail.len().saturating_sub(END.len());
                    tail.drain(..keep);
                }
            }
            let seconds = started.elapsed().as_secs_f64();
            for channel in channels {
                connection.detach(channel);
            }
            connection.stop(&[&terminal]);
            bytes.len() as f64 / seconds / 1e6
        },
        true,
    )
}

/// A shell line that prints `lines` lines of about 100 columns, with a
/// colour on each, then `done`.
fn history_script(lines: usize, done: &str) -> String {
    format!(
        "seq 1 {lines} | awk '{{printf \"\\033[3%dmline %d of a long history, with text to make each line about as long as agents write them\\033[0m\\n\", $1 % 8, $1}}'; echo {done}; sleep 600"
    )
}

/// Milliseconds from an attach that asks for the binary snapshot, of a
/// terminal holding 10,000 lines, to the snapshot's end.
fn attach_binary() -> f64 {
    let host = start_host();
    let mut connection = Connection::open(&host);
    let terminal = connection.spawn(&host, &history_script(10_000, "history-done"));
    connection.wait_for(&terminal, "history-done");
    let format = format!("ghostty-bin;vt-{}", pinrail_screen::ghostty::version());
    let result = best_of_three(
        || {
            let started = Instant::now();
            let channel = connection.attach(&terminal, 10_000, &[&format, "vt"]);
            loop {
                let frame = connection.read();
                if frame.channel == channel
                    && frame.kind == pinrail_host::frame::kind::META
                    && serde_json::from_slice::<Value>(&frame.payload)
                        .is_ok_and(|m| m["type"] == "snapshot_end")
                {
                    break;
                }
            }
            let took = started.elapsed().as_secs_f64() * 1_000.0;
            connection.detach(channel);
            took
        },
        false,
    );
    connection.stop(&[&terminal]);
    result
}

/// First paint of 5,000 lines of history with a full-screen program on
/// the alternate screen, which a paint shows over the primary one.
fn first_paint_alt_screen() -> f64 {
    let mut screen = filled(5_000);
    let (mut replies, mut signals) = (Vec::new(), Vec::new());
    let mut full = String::from("\x1b[?1049h\x1b[H");
    let mut r = Rng(11);
    for _ in 0..ROWS {
        let mut line = String::new();
        styled_line(&mut r, &mut line);
        full.push_str(line.trim_end());
        full.push_str("\r\n");
    }
    screen.feed(full.as_bytes(), &mut replies, &mut signals);
    best_of_three(
        || {
            let started = Instant::now();
            std::hint::black_box(screen.first_paint(5_000));
            started.elapsed().as_secs_f64() * 1_000.0
        },
        false,
    )
}

/// This process's resident memory, in bytes.
fn resident_bytes() -> f64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    let kib: f64 = String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .expect("a resident size");
    kib * 1024.0
}

/// MiB the host's process grows by with 10 terminals that each hold
/// 10,000 lines.
fn host_memory() -> f64 {
    let host = start_host();
    let mut connection = Connection::open(&host);
    // a terminal first, so that the host's own start is not counted
    let first = connection.spawn(&host, "echo ready; sleep 600");
    connection.wait_for(&first, "ready");
    std::thread::sleep(Duration::from_millis(500));
    let before = resident_bytes();
    let terminals: Vec<String> = (0..10)
        .map(|i| connection.spawn(&host, &history_script(10_000, &format!("filled-{i}"))))
        .collect();
    for (i, terminal) in terminals.iter().enumerate() {
        connection.wait_for(terminal, &format!("filled-{i}"));
    }
    std::thread::sleep(Duration::from_millis(500));
    let grown = (resident_bytes() - before) / (1024.0 * 1024.0);
    let all: Vec<&str> = terminals
        .iter()
        .chain([&first])
        .map(String::as_str)
        .collect();
    connection.stop(&all);
    grown
}

/// What failed: a budget, or the baseline by more than the tolerance.
fn check(
    results: &serde_json::Map<String, Value>,
    baseline: Option<&Value>,
    budgets: bool,
) -> Vec<String> {
    let mut failures = Vec::new();
    for measure in MEASURES {
        let Some(value) = results[measure.name].as_f64() else {
            continue;
        };
        let within = |limit: f64| {
            if measure.higher_is_better {
                value >= limit
            } else {
                value <= limit
            }
        };
        if let Some(budget) = measure.budget.filter(|_| budgets)
            && !within(budget)
        {
            failures.push(format!(
                "{}: {value:.2} {} misses the budget of {budget} {}",
                measure.name, measure.unit, measure.unit
            ));
        }
        if let Some(base) = baseline.and_then(|b| b[measure.name].as_f64()) {
            let limit = if measure.higher_is_better {
                base * (1.0 - TOLERANCE) - measure.margin
            } else {
                base * (1.0 + TOLERANCE) + measure.margin
            };
            if !within(limit) {
                failures.push(format!(
                    "{}: {value:.2} {} is worse than the baseline's {base:.2} by more than 15 %",
                    measure.name, measure.unit
                ));
            }
        }
    }
    failures
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut baseline, mut write_baseline, mut budgets) = (None, None, true);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--baseline" => baseline = args.next(),
            "--write-baseline" => write_baseline = args.next(),
            "--skip-budgets" => budgets = false,
            // cargo bench passes --bench
            _ => {}
        }
    }
    let mut r = Rng(42);
    let tui_bytes = tui(&mut r);
    let plain_bytes = plain(&mut r);
    let mut results = serde_json::Map::new();
    results.insert("throughput_tui".into(), json!(throughput(&tui_bytes)));
    results.insert("throughput_plain".into(), json!(throughput(&plain_bytes)));
    results.insert("memory_10k_lines".into(), json!(memory()));
    results.insert("first_paint_5k_lines".into(), json!(first_paint()));
    results.insert("idle_cpu_10_agents".into(), json!(idle_cpu()));
    let tui_8mb = &tui_bytes.as_bytes()[..8 << 20];
    results.insert(
        "viewer_throughput_tui".into(),
        json!(viewer_throughput(tui_8mb, 1)),
    );
    results.insert(
        "viewer_throughput_tui_3_viewers".into(),
        json!(viewer_throughput(tui_8mb, 3)),
    );
    results.insert("attach_binary_10k_lines".into(), json!(attach_binary()));
    results.insert(
        "first_paint_alt_screen_5k_lines".into(),
        json!(first_paint_alt_screen()),
    );
    results.insert("host_memory_10_terminals".into(), json!(host_memory()));

    let baseline: Option<Value> = baseline.map(|path| {
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
    });
    let failures = check(&results, baseline.as_ref(), budgets);
    let report = json!({
        "results": results,
        "units": MEASURES
            .iter()
            .map(|m| (m.name.to_string(), json!(m.unit)))
            .collect::<serde_json::Map<_, _>>(),
        "failures": failures,
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    if let Some(path) = write_baseline {
        let text = serde_json::to_string_pretty(&Value::Object(results)).unwrap();
        std::fs::write(&path, text + "\n").unwrap_or_else(|e| panic!("{path}: {e}"));
    }
    if !failures.is_empty() {
        std::process::exit(1);
    }
}
