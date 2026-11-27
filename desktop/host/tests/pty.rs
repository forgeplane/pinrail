//! A program starts as the session leader of its own PTY, in the folder
//! and environment it was given, and its exit is seen without reaping it.
//! On Windows it starts on a pseudo console of its own, in a job, and its
//! exit is always a code.

mod common;

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use pinrail_host::pty::{self, Exit, SpawnError};

use common::Local;

fn env(extra: &[(&str, &str)]) -> BTreeMap<String, String> {
    let path = common::path();
    let mut env: BTreeMap<String, String> = [("PATH", path.as_str()), ("HOME", "/tmp")]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    env.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    env
}

fn sh(script: &str, cwd: &Path, env: &BTreeMap<String, String>) -> Result<Local, SpawnError> {
    let argv = ["sh", "-c", script].map(String::from);
    common::spawn_local(&argv, cwd, env, (132, 40))
}

/// Everything the program writes until it exits and its output is read.
fn output_and_exit(local: Local) -> (String, Exit) {
    let pid = local.spawned.pid;
    let waiter = std::thread::spawn(local.wait);
    let output = read_all(&local.spawned.master, &waiter);
    let exit = waiter.join().unwrap();
    pty::reap(pid);
    (output, exit)
}

#[cfg(unix)]
fn read_all(master: &pty::Master, waiter: &std::thread::JoinHandle<Exit>) -> String {
    use std::io::Read;
    use std::time::Instant;
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

/// The console's output ends once the program has exited and its console
/// is closed, which waiting for the exit does.
#[cfg(windows)]
fn read_all(master: &pty::Master, _waiter: &std::thread::JoinHandle<Exit>) -> String {
    let (mut waker, wake) = pty::wake_pair().unwrap();
    // a program that never ends would otherwise hold the test for ever
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(20));
        waker.wake();
    });
    let mut out = Vec::new();
    pty::read_loop(master.clone(), wake, |bytes| {
        out.extend(bytes);
        true
    });
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn the_program_is_the_session_leader_of_its_own_terminal() {
    let dir = tempfile::tempdir().unwrap();
    // a pseudo console has no sessions or process groups to lead
    let (leader, lines): (_, &[&str]) = if cfg!(unix) {
        (
            "[ \"$(ps -o pgid= -p $$ | tr -d ' ')\" = \"$$\" ] && echo leader;",
            &["tty", "ok", "40 132", "leader"],
        )
    } else {
        ("", &["tty", "ok", "40 132"])
    };
    let spawned = sh(
        &format!("[ -t 0 ] && echo tty; echo ok > /dev/tty; stty size; {leader} exit 3"),
        dir.path(),
        &env(&[]),
    )
    .unwrap();
    assert!(spawned.spawned.started.is_some());
    let (output, exit) = output_and_exit(spawned);
    for line in lines {
        assert!(output.contains(line), "{line} in {output:?}");
    }
    assert_eq!(exit, Exit::Code(3));
}

#[test]
fn the_program_gets_exactly_the_folder_and_environment_given() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().canonicalize().unwrap();
    // a variable of the test's own process: the person's name on Unix; on
    // Windows, where the shell makes that up itself, one Cargo sets
    let (pwd, own) = if cfg!(windows) {
        ("pwd -W", "CARGO_PKG_NAME")
    } else {
        ("pwd", "USER")
    };
    assert!(std::env::var_os(own).is_some());
    let spawned = sh(
        &format!("{pwd}; echo \"agent=$AGENT_TEST\"; echo \"own=${{{own}-unset}}\""),
        &folder,
        &env(&[("AGENT_TEST", "given")]),
    )
    .unwrap();
    let (output, _) = output_and_exit(spawned);
    // as the shell spells it on Windows: without the prefix of a canonical
    // path, with forward slashes
    let folder = folder.to_str().unwrap();
    let folder = if cfg!(windows) {
        folder.trim_start_matches(r"\\?\").replace('\\', "/")
    } else {
        folder.to_string()
    };
    assert!(output.contains(&folder), "{folder} in {output}");
    assert!(output.contains("agent=given"));
    // nothing of the host's own environment
    assert!(output.contains("own=unset"), "{output}");
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
    let error = common::spawn_local(&argv, dir.path(), &env(&[]), (80, 24)).unwrap_err();
    assert!(matches!(error, SpawnError::NotFound(name) if name == "no-such-agent-binary"));
    // sh exists, but not on this PATH
    let error = sh("true", dir.path(), &env(&[("PATH", "/nonexistent")])).unwrap_err();
    assert!(matches!(error, SpawnError::NotFound(_)));
}

#[cfg(unix)]
#[test]
fn an_exit_by_signal_is_reported_as_one() {
    let dir = tempfile::tempdir().unwrap();
    let spawned = sh("kill -TERM $$; sleep 5", dir.path(), &env(&[])).unwrap();
    assert_eq!(output_and_exit(spawned).1, Exit::Signal(libc_sigterm()));
}

/// Windows has no signals to die of: a program that was killed, as the
/// job's end kills it, exited with a code like any other.
#[cfg(windows)]
#[test]
fn a_killed_program_s_exit_is_a_code() {
    let dir = tempfile::tempdir().unwrap();
    let spawned = sh("echo ready; sleep 30", dir.path(), &env(&[])).unwrap();
    common::read_until(&spawned.spawned.master, "ready");
    pty::signal(&spawned.spawned.master, spawned.spawned.pid, pty::SIGKILL).unwrap();
    assert_eq!(output_and_exit(spawned).1, Exit::Code(1));
}

#[cfg(unix)]
#[test]
fn an_exited_program_stays_unreaped_until_reaped() {
    let dir = tempfile::tempdir().unwrap();
    let spawned = sh("exit 0", dir.path(), &env(&[])).unwrap();
    let pid = spawned.spawned.pid;
    assert_eq!(pty::wait_exit(pid).unwrap(), Exit::Code(0));
    // still waitable, so its id is still its own
    assert_eq!(pty::wait_exit(pid).unwrap(), Exit::Code(0));
    pty::reap(pid);
    assert!(pty::wait_exit(pid).is_err());
}

/// What keeps a program's id its own on Windows is the handle its holder
/// has to it, which also tells its exit any number of times.
#[cfg(windows)]
#[test]
fn an_exited_program_s_code_is_read_as_long_as_it_is_held() {
    let dir = tempfile::tempdir().unwrap();
    let argv = ["sh", "-c", "exit 6"].map(String::from);
    let holding = pty::Holding::spawn(&argv, dir.path(), &env(&[]), 80, 24).unwrap();
    assert_eq!(holding.exit_waiter().unwrap()().unwrap(), Exit::Code(6));
    assert_eq!(holding.exit_waiter().unwrap()().unwrap(), Exit::Code(6));
    holding.forget(true);
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
    // wait for the trap to be set
    common::read_until(&spawned.spawned.master, "ready");
    assert_eq!(
        pty::foreground_group(&spawned.spawned.master),
        Some(spawned.spawned.pid)
    );
    pty::resize(&spawned.spawned.master, 100, 30).unwrap();
    let (output, exit) = output_and_exit(spawned);
    assert!(output.contains("30 100"), "{output}");
    assert_eq!(exit, Exit::Code(7));

    #[cfg(unix)]
    {
        let spawned = sh("sleep 30", dir.path(), &env(&[])).unwrap();
        pty::signal_group(spawned.spawned.pid, libc_sigterm()).unwrap();
        assert_eq!(output_and_exit(spawned).1, Exit::Signal(libc_sigterm()));
    }
    // a terminate closes the pseudo console, and the program ends as its
    // console goes, once it has one: a program still starting is told
    // nothing
    #[cfg(windows)]
    {
        let spawned = sh("echo ready; sleep 30", dir.path(), &env(&[])).unwrap();
        common::read_until(&spawned.spawned.master, "ready");
        pty::signal(&spawned.spawned.master, spawned.spawned.pid, pty::SIGTERM).unwrap();
        assert!(matches!(output_and_exit(spawned).1, Exit::Code(code) if code != 0));
    }
}

#[cfg(unix)]
fn libc_sigterm() -> i32 {
    15
}
