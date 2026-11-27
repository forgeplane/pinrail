//! Starting a real `pinrail-host serve` with folders of the test's own.
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use pinrail_host::registration::Registration;

/// A data folder and a runtime folder of the test's own.
pub struct Folders {
    _root: tempfile::TempDir,
    pub data: PathBuf,
    pub runtime: PathBuf,
    /// The host's home folder, whose `.profile` the login probe reads.
    pub home: PathBuf,
}

pub fn folders() -> Folders {
    // short, so that the socket path fits; pipes have no such limit
    let parent = if cfg!(windows) {
        std::env::temp_dir()
    } else {
        PathBuf::from("/tmp")
    };
    let root = tempfile::Builder::new()
        .prefix("ph")
        .tempdir_in(parent)
        .unwrap();
    let data = root.path().join("data");
    let runtime = root.path().join("run");
    std::fs::create_dir(&runtime).unwrap();
    let home = root.path().join("home");
    std::fs::create_dir(&home).unwrap();
    Folders {
        home,
        data,
        runtime,
        _root: root,
    }
}

/// How long a test waits for what a program does: `seconds`, and five
/// times as long on Windows, where Git's shell takes seconds to start and
/// to start each program, and more while other tests run.
pub fn within(seconds: u64) -> Duration {
    Duration::from_secs(if cfg!(windows) { seconds * 5 } else { seconds })
}

/// The folders of the programs the tests run, as a `PATH`: on Windows,
/// the POSIX tools Git for Windows brings, then Windows' own.
pub fn path() -> String {
    if cfg!(windows) {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
        format!(r"{};{root}\System32", posix_tools().display())
    } else {
        "/usr/bin:/bin".to_string()
    }
}

/// Where Git for Windows keeps `sh` and the tools the tests' scripts use.
fn posix_tools() -> PathBuf {
    let programs =
        std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".to_string());
    PathBuf::from(programs).join(r"Git\usr\bin")
}

/// The arguments of a POSIX shell that runs `script`. On Windows it is
/// Git for Windows' `sh`, named by its path, with its tools' folder put on
/// the script's `PATH` first: whatever environment the terminal starts
/// with, the script finds `sleep`, `stty` and the rest.
pub fn shell(script: &str) -> Vec<String> {
    if cfg!(windows) {
        let sh = posix_tools().join("sh.exe");
        vec![
            sh.to_string_lossy().into_owned(),
            "-c".to_string(),
            format!("PATH=/usr/bin:$PATH; {script}"),
        ]
    } else {
        ["sh", "-c", script].map(String::from).to_vec()
    }
}

/// The arguments of a program that runs for `seconds` and does nothing
/// else: `sleep`, or on Windows a `ping` of this computer, once a second.
/// Git for Windows has a `sleep`, but every program of its kind takes one
/// of the 128 consoles their runtime shares, which the many terminals of
/// one test would use up for all the others.
pub fn sleep(seconds: u32) -> Vec<String> {
    if cfg!(windows) {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
        vec![
            format!(r"{root}\System32\ping.exe"),
            "-n".to_string(),
            (seconds + 1).to_string(),
            "127.0.0.1".to_string(),
        ]
    } else {
        vec!["sleep".to_string(), seconds.to_string()]
    }
}

/// Whether the process `pid` exists: on Unix, until it is reaped.
#[cfg(unix)]
pub fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Whether the process `pid` runs.
#[cfg(windows)]
pub fn alive(pid: u32) -> bool {
    pinrail_runtime::process::process(pid).is_some()
}

/// Kills the process `pid`.
#[cfg(unix)]
pub fn kill(pid: u32) {
    let _ = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .stderr(Stdio::null())
        .status();
}

/// Kills the process `pid`.
#[cfg(windows)]
pub fn kill(pid: u32) {
    if let Some(process) = pinrail_runtime::process::process(pid) {
        pinrail_runtime::process::stop(pid, &process.started, true);
    }
}

/// A copy of the host's binary in `dir`, which stands for another build,
/// with the ConPTY it loads from beside it on Windows.
pub fn host_copy(dir: &std::path::Path) -> PathBuf {
    let binary = std::path::Path::new(env!("CARGO_BIN_EXE_pinrail-host"));
    let copy = dir.join(binary.file_name().unwrap());
    std::fs::copy(binary, &copy).unwrap();
    let conpty = binary.with_file_name("conpty");
    if cfg!(windows) && conpty.is_dir() {
        copy_folder(&conpty, &dir.join("conpty"));
    }
    copy
}

fn copy_folder(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let copy = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_folder(&entry.path(), &copy);
        } else {
            std::fs::copy(entry.path(), &copy).unwrap();
        }
    }
}

/// A program started in a terminal without a host or a holder: the test
/// is its parent, and does for it what a holder would.
pub struct Local {
    pub spawned: pinrail_host::pty::Spawned,
    /// Waits until the program has exited, and leaves it unreaped.
    pub wait: Box<dyn FnOnce() -> pinrail_host::pty::Exit + Send>,
    /// Lets the program go, once nothing signals it any more.
    pub forget: Box<dyn FnOnce() + Send>,
}

impl std::fmt::Debug for Local {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.spawned.fmt(f)
    }
}

#[cfg(unix)]
pub fn spawn_local(
    argv: &[String],
    cwd: &std::path::Path,
    env: &std::collections::BTreeMap<String, String>,
    (cols, rows): (u16, u16),
) -> Result<Local, pinrail_host::pty::SpawnError> {
    use pinrail_host::pty;
    let spawned = pty::spawn(argv, cwd, env, cols, rows)?;
    let pid = spawned.pid;
    Ok(Local {
        spawned,
        wait: Box::new(move || pty::wait_exit(pid).unwrap_or(pty::Exit::Code(-1))),
        forget: Box::new(move || pty::reap(pid)),
    })
}

/// On Windows the test keeps the pseudo console, as a holder does: it
/// resizes and closes it when the terminal asks, and closes it once the
/// program has exited, so that the output ends.
#[cfg(windows)]
pub fn spawn_local(
    argv: &[String],
    cwd: &std::path::Path,
    env: &std::collections::BTreeMap<String, String>,
    (cols, rows): (u16, u16),
) -> Result<Local, pinrail_host::pty::SpawnError> {
    use pinrail_host::pty;
    let holding = std::sync::Arc::new(pty::Holding::spawn(argv, cwd, env, cols, rows)?);
    let pid = holding.pid;
    let master = pty::master_from(pid, &mut holding.for_host()?.into_iter(), (cols, rows))
        .expect("the console's input, its output and the job");
    let asked = holding.clone();
    pty::set_control(
        &master,
        Box::new(move |request| match request["op"].as_str() {
            Some("resize") => {
                let size = |key: &str| request[key].as_u64().unwrap() as u16;
                asked.resize(size("cols"), size("rows")).unwrap();
            }
            Some("close") => asked.close(),
            other => panic!("the terminal asked its holder for {other:?}"),
        }),
    );
    let waiter = holding.exit_waiter()?;
    let (waited, forgotten) = (holding.clone(), holding.clone());
    Ok(Local {
        spawned: pty::Spawned {
            pid,
            started: holding.started.clone(),
            master,
        },
        wait: Box::new(move || {
            let exit = waiter().unwrap_or(pty::Exit::Code(-1));
            waited.exited();
            exit
        }),
        forget: Box::new(move || forgotten.close()),
    })
}

/// Reads a terminal's output until `wanted` appears in it.
#[cfg(unix)]
pub fn read_until(master: &pinrail_host::pty::Master, wanted: &str) {
    use std::io::Read;
    let mut file = std::fs::File::from(master.try_clone().unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = Vec::new();
    let mut buf = [0u8; 1024];
    while !String::from_utf8_lossy(&seen).contains(wanted) {
        assert!(
            Instant::now() < deadline,
            "{:?}",
            String::from_utf8_lossy(&seen)
        );
        match file.read(&mut buf) {
            Ok(n) if n > 0 => seen.extend(&buf[..n]),
            _ => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// Reads a terminal's output until `wanted` appears in it.
#[cfg(windows)]
pub fn read_until(master: &pinrail_host::pty::Master, wanted: &str) {
    let (mut waker, wake) = pinrail_host::pty::wake_pair().unwrap();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(10));
        waker.wake();
    });
    let mut seen = Vec::new();
    pinrail_host::pty::read_loop(master.clone(), wake, |bytes| {
        seen.extend(bytes);
        !String::from_utf8_lossy(&seen).contains(wanted)
    });
    let seen = String::from_utf8_lossy(&seen);
    assert!(seen.contains(wanted), "{seen:?}");
}

/// Types `bytes` into a terminal.
pub fn type_into(master: &pinrail_host::pty::Master, bytes: &[u8]) {
    let (input, typed) = std::sync::mpsc::channel();
    input.send(bytes.to_vec()).unwrap();
    drop(input);
    pinrail_host::pty::write_loop(master.try_clone().unwrap(), typed);
}

pub fn command(folders: &Folders, args: &[&str]) -> Command {
    command_of(
        std::path::Path::new(env!("CARGO_BIN_EXE_pinrail-host")),
        folders,
        args,
    )
}

/// A command of the host at `binary`, such as a copy that stands for
/// another build.
pub fn command_of(binary: &std::path::Path, folders: &Folders, args: &[&str]) -> Command {
    // On Windows a process inherits what its parent can hand down, which
    // would be the test's own output: the host, its holder and each
    // terminal's console would keep it open, and whatever runs the test
    // would wait for them once the test is done.
    pinrail_host::keep_own_output_from_children();
    let mut command = Command::new(binary);
    command
        .args(args)
        .arg("--data-dir")
        .arg(&folders.data)
        .env("TMPDIR", &folders.runtime)
        .env("XDG_RUNTIME_DIR", &folders.runtime)
        // a login shell of the test's own, not the person's
        .env("SHELL", "/bin/sh")
        .env("HOME", &folders.home)
        .stdin(Stdio::null());
    command
}

pub fn run(folders: &Folders, args: &[&str]) -> Output {
    command(folders, args).output().unwrap()
}

/// A running `serve`, killed when dropped with the holders it started,
/// which would otherwise keep the test's programs running.
pub struct Host {
    pub child: Child,
    pub registration: Registration,
    pub holders: PathBuf,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        kill_holders(&self.holders);
    }
}

/// Kills the holders whose locks are in `dir`, by the pids they wrote. On
/// Windows a lock cannot be read while it is held, so there they are found
/// by their data folder, and the hosts a holder started go with them.
#[cfg(windows)]
pub fn kill_holders(dir: &std::path::Path) {
    // `<data>/host/holders`, as it is when the host has made a holder
    let Some(data) = dir.is_dir().then(|| dir.parent()?.parent()).flatten() else {
        return;
    };
    let script = format!(
        "Get-CimInstance Win32_Process -Filter \"Name='pinrail-host.exe'\" | \
         Where-Object {{ $_.CommandLine -like '*{}*' }} | \
         ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force }}",
        data.display()
    );
    let _ = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Kills the holders whose locks are in `dir`, by the pids they wrote.
#[cfg(unix)]
pub fn kill_holders(dir: &std::path::Path) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if let Some(pid) = std::fs::read_to_string(entry.path())
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
        {
            kill(pid);
        }
    }
}

pub fn registration_path(folders: &Folders) -> PathBuf {
    folders.data.join("host/host.json")
}

pub fn serve(folders: &Folders, extra: &[&str]) -> Host {
    serve_of(
        std::path::Path::new(env!("CARGO_BIN_EXE_pinrail-host")),
        folders,
        extra,
    )
}

/// A running `serve` of the host at `binary`.
pub fn serve_of(binary: &std::path::Path, folders: &Folders, extra: &[&str]) -> Host {
    let mut args = vec!["serve"];
    args.extend(extra);
    started(command_of(binary, folders, &args), folders)
}

/// The host `serve` starts, once it has registered.
pub fn started(mut serve: Command, folders: &Folders) -> Host {
    let mut child = serve
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let path = registration_path(folders);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(registration) = Registration::read(&path)
            && registration.pid == child.id()
        {
            return Host {
                child,
                registration,
                holders: folders.data.join("host/holders"),
            };
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("the host exited with {status}");
        }
        assert!(Instant::now() < deadline, "the host did not register");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Waits for the process to exit and returns whether it exited cleanly.
pub fn exits_within(host: &mut Host, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if let Some(status) = host.child.try_wait().unwrap() {
            return status.success();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}
