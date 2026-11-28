//! The host's side of the holders: it finds the holders that run, spawns
//! each terminal's program through one, and hears from them when a program
//! exits. The holder keeps the PTYs open, so the programs outlive this
//! host.
//!
//! A program belongs to the holder that spawned it, which is what macOS
//! names in a privacy prompt and checks a grant against. So the host
//! spawns only through a holder of its own build, and starts one when none
//! runs. A holder of another build, as one left by the host before an
//! update, keeps the terminals it has, and the host closes its connection
//! when the last of them ends, which lets that holder exit.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use serde_json::{Value, json};

use crate::fdpass::{self, Owned};
use crate::holder;
use crate::net::Stream;
use crate::paths::Paths;
use crate::pty::{self, Exit, SpawnError, Spawned};

/// How long a holder has to answer a request.
const ANSWER_WITHIN: Duration = Duration::from_secs(10);

/// How the host starts a holder when none runs.
#[derive(Debug, Clone)]
pub enum Start {
    /// `<binary> hold`, in a session of its own, so that it outlives the
    /// host, and exits `idle_exit` after it has neither terminals nor a
    /// host.
    Process {
        binary: PathBuf,
        idle_exit: Duration,
    },
    /// On a thread of this process, for a host that runs inside a test:
    /// its programs end with the process.
    Thread,
}

/// A program's exit, as a holder reports it: the holder's instance, the
/// terminal and the exit.
/// The terminals a holder lists in one message: their descriptors, two
/// or four each, stay well within what a message carries.
const LIST_PAGE: usize = 32;

pub type Exited = (String, String, Exit);

type Pending = Arc<Mutex<HashMap<u64, mpsc::Sender<(Value, Vec<Owned>)>>>>;

/// Why a request to a holder has no answer.
#[derive(Debug)]
enum Unanswered {
    /// The holder has gone, or the connection to it broke: it did not do
    /// what was asked, or did not live to keep it.
    Gone(std::io::Error),
    /// The holder is there and did not answer in time. It may still do
    /// what was asked.
    Late,
}

impl From<Unanswered> for std::io::Error {
    fn from(unanswered: Unanswered) -> Self {
        match unanswered {
            Unanswered::Gone(error) => error,
            Unanswered::Late => {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "the holder did not answer")
            }
        }
    }
}

/// A connection to one holder.
struct Link {
    instance: String,
    stream: Stream,
    /// Held for each message sent: requests go from many threads, and a
    /// message written in parts must not have another's between them.
    sending: Mutex<()>,
    pending: Pending,
    next_id: AtomicU64,
    /// Cleared when the holder goes away.
    alive: Arc<AtomicBool>,
    /// Whether the holder runs this host's build.
    own: bool,
    /// How many terminals of this host the holder keeps.
    terminals: AtomicUsize,
}

impl Link {
    /// The link over `stream`, a connection to the holder `instance` past
    /// its handshake, and the thread that reads what the holder sends.
    fn over(
        instance: &str,
        stream: Stream,
        own: bool,
        exits: tokio::sync::mpsc::UnboundedSender<Exited>,
    ) -> std::io::Result<Arc<Link>> {
        let link = Arc::new(Link {
            instance: instance.to_string(),
            stream: stream.try_clone()?,
            sending: Mutex::new(()),
            pending: Pending::default(),
            next_id: AtomicU64::new(1),
            alive: Arc::new(AtomicBool::new(true)),
            own,
            terminals: AtomicUsize::new(0),
        });
        let (pending, instance, alive) = (
            link.pending.clone(),
            instance.to_string(),
            link.alive.clone(),
        );
        std::thread::Builder::new()
            .name("holder-reader".to_string())
            .spawn(move || {
                read(&stream, &pending, &exits, &instance);
                // in this order: a request that adds itself after the
                // clearing sees that the holder has gone
                alive.store(false, Ordering::SeqCst);
                // no answer comes now: those that wait for one stop
                pending.lock().unwrap_or_else(|e| e.into_inner()).clear();
            })?;
        Ok(link)
    }

    /// Closes the connection, which leaves the holder without a host.
    fn close(&self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }

    /// Sends one message whole.
    fn send(&self, value: &Value, fds: &[fdpass::Raw]) -> std::io::Result<()> {
        let _whole = self.sending.lock().unwrap_or_else(|e| e.into_inner());
        fdpass::send(&self.stream, value, fds)
    }

    /// Sends a request, and waits for its answer. It fails with
    /// [`Unanswered::Gone`] as soon as the holder is known to have gone,
    /// and with [`Unanswered::Late`] when a holder that is still there has
    /// not answered in [`ANSWER_WITHIN`].
    fn request(
        &self,
        mut request: Value,
        fds: &[fdpass::Raw],
    ) -> Result<(Value, Vec<Owned>), Unanswered> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        request["id"] = json!(id);
        let (tx, rx) = mpsc::channel();
        let pending = || self.pending.lock().unwrap_or_else(|e| e.into_inner());
        pending().insert(id, tx);
        // the reader clears what waits after it marks the holder gone: a
        // request added after that would wait for nothing
        if !self.alive.load(Ordering::SeqCst) {
            pending().remove(&id);
            return Err(Unanswered::Gone(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "the holder has gone",
            )));
        }
        // a message that failed may be there in part, and nothing after
        // it on this connection can be read
        if let Err(error) = self.send(&request, fds) {
            pending().remove(&id);
            return Err(Unanswered::Gone(error));
        }
        match rx.recv_timeout(ANSWER_WITHIN) {
            Ok(answer) => Ok(answer),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(Unanswered::Gone(
                std::io::Error::new(std::io::ErrorKind::BrokenPipe, "the holder has gone"),
            )),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                pending().remove(&id);
                Err(Unanswered::Late)
            }
        }
    }

    /// Takes the holder for gone: nothing more is asked of it, and the
    /// host chooses another for its next terminal.
    fn give_up(&self) {
        self.alive.store(false, Ordering::SeqCst);
        self.close();
    }

    /// How a terminal's master reaches this holder, for what the host
    /// cannot do to the terminal itself: a request about `terminal`, sent
    /// without waiting for its answer.
    fn control(self: &Arc<Self>, terminal: &str) -> pty::Control {
        let (link, terminal) = (Arc::downgrade(self), terminal.to_string());
        Box::new(move |mut request: Value| {
            let Some(link) = link.upgrade() else {
                return;
            };
            request["terminal"] = json!(terminal);
            request["id"] = json!(link.next_id.fetch_add(1, Ordering::Relaxed));
            let _ = link.send(&request, &[]);
        })
    }
}

/// A terminal a holder keeps, as a host that starts takes it back.
pub struct Kept {
    pub holder: String,
    pub terminal: String,
    pub spawned: Spawned,
    pub ring: Option<Owned>,
    /// How the program ended, if it has.
    pub exit: Option<Exit>,
}

/// The holders this host talks to.
pub struct Holders {
    paths: Paths,
    start: Start,
    links: Mutex<BTreeMap<String, Arc<Link>>>,
    exits: tokio::sync::mpsc::UnboundedSender<Exited>,
}

impl Holders {
    /// The holders of `paths`, whose exits go to `exits`. Nothing is
    /// connected until a terminal needs it.
    pub fn new(
        paths: Paths,
        start: Start,
        exits: tokio::sync::mpsc::UnboundedSender<Exited>,
    ) -> Self {
        Holders {
            paths,
            start,
            links: Mutex::new(BTreeMap::new()),
            exits,
        }
    }

    fn secret(&self) -> std::io::Result<String> {
        crate::registration::Registration::read(&self.paths.registration())
            .map(|r| r.secret)
            .ok_or_else(|| std::io::Error::other("the host's registration is missing"))
    }

    /// Whether the holder that runs as the process `pid` is of this host's
    /// build: it runs the binary this host starts a holder from.
    fn own(&self, pid: Option<u64>) -> bool {
        let Some(pid) = pid.and_then(|pid| u32::try_from(pid).ok()) else {
            return false;
        };
        match &self.start {
            Start::Thread => pid == std::process::id(),
            Start::Process { binary, .. } => {
                let binary = binary.canonicalize().ok();
                binary.is_some()
                    && crate::sys::executable_of(pid).and_then(|exe| exe.canonicalize().ok())
                        == binary
            }
        }
    }

    /// Connects to the holder `instance`.
    fn connect(&self, instance: &str) -> std::io::Result<Arc<Link>> {
        let socket = self.paths.holder_socket(instance);
        let stream = crate::net::connect(&socket)?;
        fdpass::send(
            &stream,
            &json!({"secret": self.secret()?, "protocol": holder::PROTOCOL}),
            &[],
        )?;
        let Some((hello, _)) = fdpass::recv(&stream)? else {
            return Err(std::io::Error::other("the holder closed the connection"));
        };
        if hello["protocol"] != holder::PROTOCOL {
            return Err(std::io::Error::other(format!(
                "the holder speaks another protocol: {hello}"
            )));
        }
        Link::over(
            instance,
            stream,
            self.own(hello["pid"].as_u64()),
            self.exits.clone(),
        )
    }

    /// A holder to spawn with, of this host's build: one this host is
    /// connected to, else one that runs, else a new one.
    fn holder(&self) -> std::io::Result<Arc<Link>> {
        let mut links = self.links.lock().unwrap_or_else(|e| e.into_inner());
        links.retain(|_, link| link.alive.load(Ordering::Relaxed));
        if let Some(link) = links.values().rev().find(|link| link.own) {
            return Ok(link.clone());
        }
        for instance in self.paths.holder_instances() {
            if links.contains_key(&instance) {
                continue;
            }
            let Ok(link) = self.connect(&instance) else {
                continue;
            };
            if link.own {
                links.insert(instance, link.clone());
                return Ok(link);
            }
            link.close();
        }
        let instance = self.start_holder()?;
        let link = self.connect(&instance)?;
        if !link.own {
            link.close();
            return Err(std::io::Error::other(
                "the holder this host started is not of its build",
            ));
        }
        links.insert(instance, link.clone());
        Ok(link)
    }

    /// Starts a holder, and returns its instance id.
    fn start_holder(&self) -> std::io::Result<String> {
        match &self.start {
            Start::Thread => {
                let data_dir = self.paths.host_dir.parent().unwrap_or(&self.paths.host_dir);
                let listening = holder::listen(data_dir)?;
                let instance = listening.instance().to_string();
                // a host inside a test is never restarted by its holder
                std::thread::spawn(move || listening.run(Duration::MAX, None));
                Ok(instance)
            }
            Start::Process { binary, idle_exit } => start_process(binary, *idle_exit, &self.paths),
        }
    }

    /// Spawns a program through a holder. Returns the holder's instance
    /// and the program, whose master descriptor the host now shares.
    pub fn spawn(
        &self,
        terminal: &str,
        argv: &[String],
        cwd: &str,
        env: &BTreeMap<String, String>,
        (cols, rows): (u16, u16),
        ring: Option<&Owned>,
    ) -> Result<(String, Spawned), SpawnError> {
        let request = json!({"op": "spawn", "terminal": terminal, "argv": argv, "cwd": cwd,
                             "env": env, "cols": cols, "rows": rows});
        let ring: Vec<fdpass::Raw> = ring.map(fdpass::raw).into_iter().collect();
        let mut link = self.holder()?;
        let answered = match link.request(request.clone(), &ring) {
            // the holder died, and the reader had not seen it yet when it
            // was chosen: once more, with a holder that runs. A holder
            // that is only late may still spawn the program, so it is not
            // asked twice.
            Err(Unanswered::Gone(_)) => {
                link.give_up();
                link = self.holder()?;
                link.request(request, &ring)
            }
            answered => answered,
        };
        let (answer, fds) = answered.map_err(|unanswered| {
            if matches!(unanswered, Unanswered::Gone(_)) {
                link.give_up();
            }
            std::io::Error::from(unanswered)
        })?;
        match answer["error"].as_str() {
            Some("folder_missing") => return Err(SpawnError::FolderMissing),
            Some("not_found") => {
                return Err(SpawnError::NotFound(
                    argv.first().cloned().unwrap_or_default(),
                ));
            }
            Some(_) => {
                return Err(SpawnError::Io(std::io::Error::other(
                    answer["message"]
                        .as_str()
                        .unwrap_or("the spawn failed")
                        .to_string(),
                )));
            }
            None => {}
        }
        let pid = answer["pid"].as_u64().unwrap_or_default() as u32;
        let master = pty::master_from(pid, &mut fds.into_iter(), (cols, rows))
            .ok_or_else(|| std::io::Error::other("the holder sent no descriptor"))?;
        pty::set_control(&master, link.control(terminal));
        link.terminals.fetch_add(1, Ordering::Relaxed);
        Ok((
            link.instance.clone(),
            Spawned {
                pid,
                started: answer["started"].as_str().map(str::to_string),
                master,
            },
        ))
    }

    /// Connects to every holder that runs, and takes back the terminals
    /// they keep, with their master and ring descriptors: what a host that
    /// starts adopts.
    pub fn adopt(&self) -> Vec<Kept> {
        let mut kept = Vec::new();
        let mut links = self.links.lock().unwrap_or_else(|e| e.into_inner());
        for instance in self.paths.holder_instances() {
            let Ok(link) = self.connect(&instance) else {
                continue;
            };
            // a page at a time: one message carries the descriptors of some
            // 80 terminals at most. A holder of an earlier build answers
            // with them all, and no more after.
            let mut pages = Vec::new();
            let mut after: Option<String> = None;
            loop {
                let mut request = json!({"op": "list", "limit": LIST_PAGE});
                if let Some(after) = &after {
                    request["after"] = json!(after);
                }
                let Ok((listed, fds)) = link.request(request, &[]) else {
                    break;
                };
                let last = listed["terminals"]
                    .as_array()
                    .and_then(|t| t.last())
                    .and_then(|t| t["terminal"].as_str())
                    .map(str::to_string);
                let more = listed["more"] == true && last.is_some();
                pages.push((listed, fds));
                if !more {
                    break;
                }
                after = last;
            }
            for (listed, fds) in pages {
                let mut fds = fds.into_iter();
                for entry in listed["terminals"].as_array().into_iter().flatten() {
                    let pid = entry["pid"].as_u64().unwrap_or_default() as u32;
                    let size = |key: &str, default: u16| {
                        entry[key]
                            .as_u64()
                            .and_then(|n| u16::try_from(n).ok())
                            .unwrap_or(default)
                    };
                    let size = (size("cols", 80), size("rows", 24));
                    let Some(master) = pty::master_from(pid, &mut fds, size) else {
                        break;
                    };
                    let terminal = entry["terminal"].as_str().unwrap_or_default();
                    pty::set_control(&master, link.control(terminal));
                    let ring = if entry["ring"] == true {
                        fds.next()
                    } else {
                        None
                    };
                    let exit = match (&entry["exit"]["code"], &entry["exit"]["signal"]) {
                        (Value::Number(code), _) => {
                            Some(Exit::Code(code.as_i64().unwrap_or(-1) as i32))
                        }
                        (_, Value::Number(signal)) => {
                            Some(Exit::Signal(signal.as_i64().unwrap_or(-1) as i32))
                        }
                        _ => None,
                    };
                    link.terminals.fetch_add(1, Ordering::Relaxed);
                    kept.push(Kept {
                        holder: instance.clone(),
                        terminal: entry["terminal"].as_str().unwrap_or_default().to_string(),
                        spawned: Spawned {
                            pid,
                            started: entry["started"].as_str().map(str::to_string),
                            master,
                        },
                        ring,
                        exit,
                    });
                }
            }
            // a holder of another build with nothing to keep may exit
            if !link.own && link.terminals.load(Ordering::Relaxed) == 0 {
                link.close();
                continue;
            }
            links.insert(instance, link);
        }
        kept
    }

    /// Tells the holder `instance` that the host is done with `terminal`:
    /// it closes the descriptors and reaps the program. It does not wait.
    /// A holder of another build that keeps no other terminal of this host
    /// is then left, so that it exits.
    pub fn forget(&self, instance: &str, terminal: &str) {
        let mut links = self.links.lock().unwrap_or_else(|e| e.into_inner());
        let Some(link) = links.get(instance) else {
            return;
        };
        let id = link.next_id.fetch_add(1, Ordering::Relaxed);
        let _ = link.send(
            &json!({"op": "forget", "id": id, "terminal": terminal}),
            &[],
        );
        let left = link
            .terminals
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_sub(1))
            })
            .map_or(0, |before| before.saturating_sub(1));
        if !link.own && left == 0 {
            link.close();
            links.remove(instance);
        }
    }
}

/// Reads a holder's answers and events until it goes away.
fn read(
    stream: &Stream,
    pending: &Pending,
    exits: &tokio::sync::mpsc::UnboundedSender<Exited>,
    instance: &str,
) {
    while let Ok(Some((message, fds))) = fdpass::recv(stream) {
        if message["event"] == "exited" {
            let exit = match (message["code"].as_i64(), message["signal"].as_i64()) {
                (Some(code), _) => Exit::Code(code as i32),
                (None, Some(signal)) => Exit::Signal(signal as i32),
                _ => Exit::Code(-1),
            };
            let terminal = message["terminal"].as_str().unwrap_or_default().to_string();
            let _ = exits.send((instance.to_string(), terminal, exit));
            continue;
        }
        let Some(id) = message["id"].as_u64() else {
            continue;
        };
        let waiting = pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        if let Some(waiting) = waiting {
            let _ = waiting.send((message, fds));
        }
    }
}

/// Starts `<binary> hold` in a session of its own, detached from the
/// host's process group, and reads the instance id it prints. On macOS the
/// holder is responsible for itself, and so for the programs in its
/// terminals, as a terminal app is for its shells.
#[cfg(target_os = "macos")]
fn start_process(
    binary: &std::path::Path,
    idle_exit: Duration,
    paths: &Paths,
) -> std::io::Result<String> {
    use std::io::BufRead;
    let data_dir = paths.host_dir.parent().unwrap_or(&paths.host_dir);
    let (pid, stdout) = crate::sys::spawn_responsible(
        binary,
        &[
            "hold".as_ref(),
            "--data-dir".as_ref(),
            data_dir.as_os_str(),
            "--idle-exit".as_ref(),
            idle_exit.as_secs().to_string().as_ref(),
        ],
    )?;
    // reaped by this host while it runs, by init once it has gone
    std::thread::spawn(move || crate::sys::wait(pid));
    let mut line = String::new();
    std::io::BufReader::new(stdout).read_line(&mut line)?;
    started(line)
}

/// Starts `<binary> hold` in a session of its own, detached from the
/// host's process group, and reads the instance id it prints.
#[cfg(all(unix, not(target_os = "macos")))]
#[allow(unsafe_code)]
fn start_process(
    binary: &std::path::Path,
    idle_exit: Duration,
    paths: &Paths,
) -> std::io::Result<String> {
    use std::io::BufRead;
    use std::os::unix::process::CommandExt;
    let data_dir = paths.host_dir.parent().unwrap_or(&paths.host_dir);
    let mut command = std::process::Command::new(binary);
    command
        .arg("hold")
        .arg("--data-dir")
        .arg(data_dir)
        .arg("--idle-exit")
        .arg(idle_exit.as_secs().to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    // SAFETY: setsid is async-signal-safe and allocates nothing.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    let stdout = child.stdout.take();
    // reaped by this host while it runs, by init once it has gone
    std::thread::spawn(move || child.wait());
    let stdout = stdout.ok_or_else(|| std::io::Error::other("no output from the holder"))?;
    let mut line = String::new();
    std::io::BufReader::new(stdout).read_line(&mut line)?;
    started(line)
}

/// Starts `<binary> hold`, detached from the host, and reads the instance
/// id it prints.
#[cfg(windows)]
fn start_process(
    binary: &std::path::Path,
    idle_exit: Duration,
    paths: &Paths,
) -> std::io::Result<String> {
    use std::io::BufRead;
    let data_dir = paths.host_dir.parent().unwrap_or(&paths.host_dir);
    let mut command = std::process::Command::new(binary);
    command
        .arg("hold")
        .arg("--data-dir")
        .arg(data_dir)
        .arg("--idle-exit")
        .arg(idle_exit.as_secs().to_string())
        .stdout(std::process::Stdio::piped());
    let mut child = crate::sys::spawn_detached(command)?;
    let stdout = child.stdout.take();
    std::thread::spawn(move || child.wait());
    let stdout = stdout.ok_or_else(|| std::io::Error::other("no output from the holder"))?;
    let mut line = String::new();
    std::io::BufReader::new(stdout).read_line(&mut line)?;
    started(line)
}

/// The instance id a starting holder printed.
fn started(line: String) -> std::io::Result<String> {
    let instance = line.trim().to_string();
    if instance.is_empty() {
        return Err(std::io::Error::other("the holder did not start"));
    }
    Ok(instance)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two connected ends of the host's kind of stream.
    fn pair() -> (Stream, Stream, Option<tempfile::TempDir>) {
        #[cfg(unix)]
        let (path, dir) = {
            let dir = tempfile::tempdir().unwrap();
            (dir.path().join("s"), Some(dir))
        };
        #[cfg(windows)]
        let (path, dir) = (
            PathBuf::from(format!(
                r"\\.\pipe\pinrail-link-test-{}",
                crate::random_hex(4)
            )),
            None,
        );
        let listener = crate::net::listen(&path).unwrap();
        let ours = crate::net::connect(&path).unwrap();
        let (theirs, _) = listener.accept().unwrap();
        (ours, theirs, dir)
    }

    #[test]
    fn requests_sent_at_once_from_many_threads_reach_the_holder_whole() {
        const THREADS: usize = 4;
        const EACH: usize = 6;
        // larger than a pipe's or a socket's buffer, so a write is cut up
        const PAD: usize = 1 << 20;
        let (ours, theirs, _dir) = pair();
        let link = Arc::new(Link {
            instance: "h".into(),
            stream: ours,
            sending: Mutex::new(()),
            pending: Pending::default(),
            next_id: AtomicU64::new(1),
            alive: Arc::new(AtomicBool::new(true)),
            own: true,
            terminals: AtomicUsize::new(0),
        });
        let reader = std::thread::spawn(move || {
            let mut whole = 0;
            while whole < THREADS * EACH {
                match fdpass::recv(&theirs) {
                    Ok(Some((request, _)))
                        if request["pad"].as_str().map(str::len) == Some(PAD) =>
                    {
                        whole += 1;
                    }
                    _ => break,
                }
            }
            whole
        });
        let senders: Vec<_> = (0..THREADS)
            .map(|i| {
                let control = link.control(&format!("t{i}"));
                std::thread::spawn(move || {
                    for _ in 0..EACH {
                        control(json!({"op": "resize", "pad": "x".repeat(PAD)}));
                    }
                })
            })
            .collect();
        for sender in senders {
            sender.join().unwrap();
        }
        assert_eq!(reader.join().unwrap(), THREADS * EACH);
    }

    /// A link to a holder that takes one request and dies without an
    /// answer, as a holder that crashes does.
    fn link_to_a_holder_that_dies(instance: &str) -> (Arc<Link>, Option<tempfile::TempDir>) {
        let (ours, theirs, dir) = pair();
        let (exits, _) = tokio::sync::mpsc::unbounded_channel();
        let link = Link::over(instance, ours, true, exits).unwrap();
        std::thread::spawn(move || {
            let _ = fdpass::recv(&theirs);
            let _ = theirs.shutdown(std::net::Shutdown::Both);
        });
        (link, dir)
    }

    /// Asks the holder of `link` for something, which must fail, and long
    /// before the holder's time to answer is over.
    fn fails_at_once(link: &Link) {
        let asked = std::time::Instant::now();
        let answer = link.request(json!({"op": "list"}), &[]);
        assert!(answer.is_err());
        assert!(
            asked.elapsed() < ANSWER_WITHIN / 2,
            "waited {:?} for a holder that was gone",
            asked.elapsed()
        );
        assert!(link.pending.lock().unwrap().is_empty());
    }

    #[test]
    fn a_request_fails_at_once_when_its_holder_dies() {
        let (link, _dir) = link_to_a_holder_that_dies("h");
        fails_at_once(&link);
    }

    #[test]
    fn a_request_to_a_holder_that_has_gone_fails_at_once() {
        let (link, _dir) = link_to_a_holder_that_dies("h");
        let _ = link.request(json!({"op": "list"}), &[]);
        // whether the reader has seen the holder go by now or not
        fails_at_once(&link);
        while link.alive.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(5));
        }
        fails_at_once(&link);
    }

    /// The holders of a data folder of the test's, with the registration
    /// a host would have written.
    fn holders() -> (Holders, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(&dir.path().join("data"));
        paths.create().unwrap();
        crate::registration::Registration {
            pid: std::process::id(),
            started_at: String::new(),
            boot_id: String::new(),
            version: String::new(),
            protocol: 1,
            features: vec![],
            socket: paths.socket(),
            secret: "holders-test-secret".to_string(),
            build: None,
        }
        .write(&paths.registration())
        .unwrap();
        let (exits, _) = tokio::sync::mpsc::unbounded_channel();
        (Holders::new(paths, Start::Thread, exits), dir)
    }

    /// The holder dies between the host's choice of it and its answer:
    /// the host has not seen it go, and still takes it for alive.
    #[test]
    fn a_spawn_goes_to_another_holder_when_the_one_it_chose_dies() {
        let (holders, dir) = holders();
        let (dying, _dir) = link_to_a_holder_that_dies("dying");
        holders
            .links
            .lock()
            .unwrap()
            .insert(dying.instance.clone(), dying.clone());
        // a program that is nowhere: the answer of a holder that lives,
        // which leaves no terminal behind
        let spawned = holders.spawn(
            "t1",
            &["pinrail-no-such-program".to_string()],
            &dir.path().to_string_lossy(),
            &BTreeMap::new(),
            (80, 24),
            None,
        );
        assert!(
            matches!(spawned, Err(SpawnError::NotFound(_))),
            "{:?}",
            spawned.err().map(|e| e.to_string())
        );
        assert!(!dying.alive.load(Ordering::Relaxed));
        let links = holders.links.lock().unwrap();
        assert_eq!(links.len(), 1, "the holder the host started instead");
        assert!(!links.contains_key("dying"));
        #[cfg(unix)]
        let _ = std::fs::remove_dir_all(&holders.paths.socket_dir);
    }
}
