//! A program starts as the session leader of its own PTY, in the folder
//! and environment it was given, and its exit is seen without reaping it.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::path::Path;
use std::time::{Duration, Instant};

use pinrail_host::pty::{self, Exit, SpawnError, Spawned};

fn env(extra: &[(&str, &str)]) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = [("PATH", "/usr/bin:/bin"), ("HOME", "/tmp")]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    env.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    env
}

fn sh(script: &str, cwd: &Path, env: &BTreeMap<String, String>) -> Result<Spawned, SpawnError> {
    let argv = ["sh", "-c", script].map(String::from);
    pty::spawn(&argv, cwd, env, 132, 40)
}

/// Everything the program writes until it exits and its output is read.
fn output_and_exit(spawned: Spawned) -> (String, Exit) {
    let pid = spawned.pid;
    let waiter = std::thread::spawn(move || pty::wait_exit(pid).unwrap());
    let output = read_all(&spawned.master, &waiter);
    let exit = waiter.join().unwrap();
    pty::reap(pid);
    (output, exit)
}

fn read_all(master: &OwnedFd, waiter: &std::thread::JoinHandle<Exit>) -> String {
    let mut file = std::fs::File::from(master.try_clone().unwrap());
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut idle_after_exit = 0;
    while Instant::now() < deadline {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => out.extend(&buf[..n]),
            // EIO once the slave side is closed on Linux
            Err(e) if e.raw_os_error() == Some(5) => break,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if waiter.is_finished() {
                    idle_after_exit += 1;
                    if idle_after_exit > 20 {
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("{e}"),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn the_program_is_the_session_leader_of_its_own_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let spawned = sh(
        "[ -t 0 ] && echo tty; echo ok > /dev/tty; stty size; \
         [ \"$(ps -o pgid= -p $$ | tr -d ' ')\" = \"$$\" ] && echo leader; exit 3",
        dir.path(),
        &env(&[]),
    )
    .unwrap();
    assert!(spawned.started.is_some());
    let (output, exit) = output_and_exit(spawned);
    for line in ["tty", "ok", "40 132", "leader"] {
        assert!(output.contains(line), "{line} in {output:?}");
    }
    assert_eq!(exit, Exit::Code(3));
}

#[test]
fn the_program_gets_exactly_the_folder_and_environment_given() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().canonicalize().unwrap();
    let spawned = sh(
        "pwd; echo \"agent=$AGENT_TEST\"; echo \"user=${USER-unset}\"",
        &folder,
        &env(&[("AGENT_TEST", "given")]),
    )
    .unwrap();
    let (output, _) = output_and_exit(spawned);
    assert!(output.contains(folder.to_str().unwrap()), "{output}");
    assert!(output.contains("agent=given"));
    // nothing of the host's own environment
    assert!(output.contains("user=unset"), "{output}");
}

#[test]
fn a_missing_folder_starts_nothing() {
    let error = sh("echo started", Path::new("/no/such/folder"), &env(&[])).unwrap_err();
    assert!(matches!(error, SpawnError::FolderMissing));
    let file = tempfile::NamedTempFile::new().unwrap();
    assert!(matches!(
        sh("echo started", file.path(), &env(&[])).unwrap_err(),
        SpawnError::FolderMissing
    ));
}

#[test]
fn a_program_not_on_the_agent_s_path_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let argv = ["no-such-agent-binary".to_string()];
    let error = pty::spawn(&argv, dir.path(), &env(&[]), 80, 24).unwrap_err();
    assert!(matches!(error, SpawnError::NotFound(name) if name == "no-such-agent-binary"));
    // sh exists, but not on this PATH
    let error = sh("true", dir.path(), &env(&[("PATH", "/nonexistent")])).unwrap_err();
    assert!(matches!(error, SpawnError::NotFound(_)));
}

#[test]
fn an_exit_by_signal_is_reported_as_one() {
    let dir = tempfile::tempdir().unwrap();
    let spawned = sh("kill -TERM $$; sleep 5", dir.path(), &env(&[])).unwrap();
    assert_eq!(output_and_exit(spawned).1, Exit::Signal(libc_sigterm()));
}

#[test]
fn an_exited_program_stays_unreaped_until_reaped() {
    let dir = tempfile::tempdir().unwrap();
    let spawned = sh("exit 0", dir.path(), &env(&[])).unwrap();
    let pid = spawned.pid;
    assert_eq!(pty::wait_exit(pid).unwrap(), Exit::Code(0));
    // still waitable, so its id is still its own
    assert_eq!(pty::wait_exit(pid).unwrap(), Exit::Code(0));
    pty::reap(pid);
    assert!(pty::wait_exit(pid).is_err());
}

#[test]
fn signals_reach_the_program_s_process_group_and_resizing_sends_sigwinch() {
    let dir = tempfile::tempdir().unwrap();
    let spawned = sh(
        "trap 'stty size; exit 7' WINCH; echo ready; while :; do sleep 0.05; done",
        dir.path(),
        &env(&[]),
    )
    .unwrap();
    let mut file = std::fs::File::from(spawned.master.try_clone().unwrap());
    // wait for the trap to be set
    let mut seen = String::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !seen.contains("ready") && Instant::now() < deadline {
        let mut buf = [0u8; 256];
        if let Ok(n) = file.read(&mut buf) {
            seen.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(pty::foreground_group(&spawned.master), Some(spawned.pid));
    pty::resize(&spawned.master, 100, 30).unwrap();
    let (output, exit) = output_and_exit(spawned);
    assert!(output.contains("30 100"), "{output}");
    assert_eq!(exit, Exit::Code(7));

    let spawned = sh("sleep 30", dir.path(), &env(&[])).unwrap();
    pty::signal_group(spawned.pid, libc_sigterm()).unwrap();
    assert_eq!(output_and_exit(spawned).1, Exit::Signal(libc_sigterm()));
}

fn libc_sigterm() -> i32 {
    15
}
