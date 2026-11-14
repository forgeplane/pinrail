//! The host's side of the holders: it finds the holders that run, starts
//! one when none does, spawns each terminal's program through one, and
//! hears from them when a program exits. The holder keeps the PTYs open,
//! so the programs outlive this host.

use std::collections::{BTreeMap, HashMap};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use serde_json::{Value, json};

use crate::fdpass;
use crate::holder;
use crate::paths::Paths;
use crate::pty::{Exit, SpawnError, Spawned};

/// How long a holder has to answer a request.
const ANSWER_WITHIN: Duration = Duration::from_secs(10);

/// How the host starts a holder when none runs.
#[derive(Debug, Clone)]
pub enum Start {
    /// `<binary> hold`, in a session of its own, so that it outlives the
    /// host.
    Process(PathBuf),
    /// On a thread of this process, for a host that runs inside a test:
    /// its programs end with the process.
    Thread,
}

/// A program's exit, as a holder reports it: the holder's instance, the
/// terminal and the exit.
pub type Exited = (String, String, Exit);

type Pending = Arc<Mutex<HashMap<u64, mpsc::Sender<(Value, Vec<OwnedFd>)>>>>;

/// A connection to one holder.
struct Link {
    instance: String,
    stream: UnixStream,
    pending: Pending,
    next_id: AtomicU64,
    /// Cleared when the holder goes away.
    alive: Arc<AtomicBool>,
}

impl Link {
    /// Sends a request, and waits for its answer.
    fn request(&self, mut request: Value, fds: &[i32]) -> std::io::Result<(Value, Vec<OwnedFd>)> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        request["id"] = json!(id);
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, tx);
        fdpass::send(&self.stream, &request, fds)?;
        rx.recv_timeout(ANSWER_WITHIN)
            .map_err(|_| std::io::Error::other("the holder did not answer"))
    }
}

/// A terminal a holder keeps, as a host that starts takes it back.
pub struct Kept {
    pub holder: String,
    pub terminal: String,
    pub spawned: Spawned,
    pub ring: Option<OwnedFd>,
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

    /// Connects to the holder `instance`.
    fn connect(&self, instance: &str) -> std::io::Result<Arc<Link>> {
        let socket = holder::socket_path(&self.paths.socket_dir, instance);
        let stream = UnixStream::connect(&socket)?;
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
        let link = Arc::new(Link {
            instance: instance.to_string(),
            stream: stream.try_clone()?,
            pending: Pending::default(),
            next_id: AtomicU64::new(1),
            alive: Arc::new(AtomicBool::new(true)),
        });
        let (pending, exits, instance, alive) = (
            link.pending.clone(),
            self.exits.clone(),
            instance.to_string(),
            link.alive.clone(),
        );
        std::thread::Builder::new()
            .name("holder-reader".to_string())
            .spawn(move || {
                read(&stream, &pending, &exits, &instance);
                alive.store(false, Ordering::Relaxed);
            })?;
        Ok(link)
    }

    /// A holder to spawn with: one this host is connected to, else one
    /// that runs, else a new one.
    fn holder(&self) -> std::io::Result<Arc<Link>> {
        let mut links = self.links.lock().unwrap_or_else(|e| e.into_inner());
        links.retain(|_, link| link.alive.load(Ordering::Relaxed));
        if let Some(link) = links.values().next_back() {
            return Ok(link.clone());
        }
        for instance in holder::instances(&self.paths.socket_dir) {
            if let Ok(link) = self.connect(&instance) {
                links.insert(instance, link);
            }
        }
        if let Some(link) = links.values().next_back() {
            return Ok(link.clone());
        }
        let instance = self.start_holder()?;
        let link = self.connect(&instance)?;
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
                std::thread::spawn(move || listening.run(Duration::MAX));
                Ok(instance)
            }
            Start::Process(binary) => start_process(binary, &self.paths),
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
        ring: Option<&OwnedFd>,
    ) -> Result<(String, Spawned), SpawnError> {
        use std::os::fd::AsRawFd;
        let link = self.holder()?;
        let fds: Vec<i32> = ring.map(AsRawFd::as_raw_fd).into_iter().collect();
        let (answer, fds) = link.request(
            json!({"op": "spawn", "terminal": terminal, "argv": argv, "cwd": cwd,
                   "env": env, "cols": cols, "rows": rows}),
            &fds,
        )?;
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
        let master = fds
            .into_iter()
            .next()
            .ok_or_else(|| std::io::Error::other("the holder sent no descriptor"))?;
        Ok((
            link.instance.clone(),
            Spawned {
                pid: answer["pid"].as_u64().unwrap_or_default() as u32,
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
        for instance in holder::instances(&self.paths.socket_dir) {
            let Ok(link) = self.connect(&instance) else {
                continue;
            };
            let Ok((listed, fds)) = link.request(json!({"op": "list"}), &[]) else {
                continue;
            };
            let mut fds = fds.into_iter();
            for entry in listed["terminals"].as_array().into_iter().flatten() {
                let Some(master) = fds.next() else { break };
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
                kept.push(Kept {
                    holder: instance.clone(),
                    terminal: entry["terminal"].as_str().unwrap_or_default().to_string(),
                    spawned: Spawned {
                        pid: entry["pid"].as_u64().unwrap_or_default() as u32,
                        started: entry["started"].as_str().map(str::to_string),
                        master,
                    },
                    ring,
                    exit,
                });
            }
            links.insert(instance, link);
        }
        kept
    }

    /// Tells the holder `instance` that the host is done with `terminal`:
    /// it closes the descriptors and reaps the program. It does not wait.
    pub fn forget(&self, instance: &str, terminal: &str) {
        let links = self.links.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(link) = links.get(instance) {
            let id = link.next_id.fetch_add(1, Ordering::Relaxed);
            let _ = fdpass::send(
                &link.stream,
                &json!({"op": "forget", "id": id, "terminal": terminal}),
                &[],
            );
        }
    }
}

/// Reads a holder's answers and events until it goes away.
fn read(
    stream: &UnixStream,
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
/// host's process group, and reads the instance id it prints.
#[allow(unsafe_code)]
fn start_process(binary: &std::path::Path, paths: &Paths) -> std::io::Result<String> {
    use std::io::BufRead;
    use std::os::unix::process::CommandExt;
    let data_dir = paths.host_dir.parent().unwrap_or(&paths.host_dir);
    let mut command = std::process::Command::new(binary);
    command
        .arg("hold")
        .arg("--data-dir")
        .arg(data_dir)
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
    let mut line = String::new();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("no output from the holder"))?;
    std::io::BufReader::new(stdout).read_line(&mut line)?;
    // reaped by this host while it runs, by init once it has gone
    std::thread::spawn(move || child.wait());
    let instance = line.trim().to_string();
    if instance.is_empty() {
        return Err(std::io::Error::other("the holder did not start"));
    }
    Ok(instance)
}
