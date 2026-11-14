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
    // short, so that the socket path fits
    let root = tempfile::Builder::new()
        .prefix("ph")
        .tempdir_in("/tmp")
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

pub fn command(folders: &Folders, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pinrail-host"));
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

/// Kills the holders whose locks are in `dir`, by the pids they wrote.
pub fn kill_holders(dir: &std::path::Path) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if let Some(pid) = std::fs::read_to_string(entry.path())
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
        {
            let _ = Command::new("kill")
                .args(["-9", &pid.to_string()])
                .stderr(Stdio::null())
                .status();
        }
    }
}

pub fn registration_path(folders: &Folders) -> PathBuf {
    folders.data.join("host/host.json")
}

pub fn serve(folders: &Folders, extra: &[&str]) -> Host {
    let mut args = vec!["serve"];
    args.extend(extra);
    let mut child = command(folders, &args)
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
