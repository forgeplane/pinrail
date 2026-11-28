//! A host runs as many terminals as the person needs: here 200, well
//! beyond the 64 it used to stop at, each a program that sleeps. The test
//! prints what the host and the holder hold for them, for the docs.

mod common;

use std::process::Command;

use pinrail_host::client::Client;
use serde_json::json;

use common::{folders, serve};

const TERMINALS: usize = 200;

/// What the process `pid` holds: its open handles, its threads and its
/// working set in KiB.
#[cfg(windows)]
fn holds(pid: u32) -> (usize, usize, u64) {
    let script = format!(
        "$p = Get-Process -Id {pid}; \
         \"$($p.HandleCount) $($p.Threads.Count) $([int]($p.WorkingSet64 / 1024))\""
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .unwrap();
    let output = String::from_utf8_lossy(&output.stdout);
    let mut counts = output.split_whitespace().map(|n| n.parse().unwrap_or(0));
    let mut next = || counts.next().unwrap_or(0);
    (next() as usize, next() as usize, next())
}

/// What the process `pid` holds: its open descriptors, its threads and
/// its resident memory in KiB.
#[cfg(unix)]
fn holds(pid: u32) -> (usize, usize, u64) {
    let pid = pid.to_string();
    let fds = if cfg!(target_os = "linux") {
        std::fs::read_dir(format!("/proc/{pid}/fd"))
            .unwrap()
            .count()
    } else {
        let lsof = Command::new("lsof")
            .args(["-n", "-P", "-p", &pid])
            .output()
            .unwrap();
        // a header line, then one line per descriptor or mapping; count the
        // numbered descriptors only
        String::from_utf8_lossy(&lsof.stdout)
            .lines()
            .skip(1)
            .filter(|l| {
                l.split_whitespace()
                    .nth(3)
                    .is_some_and(|fd| fd.chars().next().is_some_and(|c| c.is_ascii_digit()))
            })
            .count()
    };
    let threads = if cfg!(target_os = "linux") {
        std::fs::read_dir(format!("/proc/{pid}/task"))
            .unwrap()
            .count()
    } else {
        let ps = Command::new("ps")
            .args(["-M", "-p", &pid])
            .output()
            .unwrap();
        String::from_utf8_lossy(&ps.stdout)
            .lines()
            .count()
            .saturating_sub(1)
    };
    let rss = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .unwrap();
    let rss = String::from_utf8_lossy(&rss.stdout)
        .trim()
        .parse()
        .unwrap_or(0);
    (fds, threads, rss)
}

/// The parent of the process `pid`.
#[cfg(windows)]
fn parent(pid: u64) -> u32 {
    pinrail_runtime::process::process(pid as u32)
        .unwrap()
        .parent
}

/// The parent of the process `pid`.
#[cfg(unix)]
fn parent(pid: u64) -> u32 {
    let ps = Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&ps.stdout).trim().parse().unwrap()
}

#[test]
fn a_host_runs_well_beyond_64_terminals() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    let host_pid = host.child.id();
    let before = holds(host_pid);

    let mut first_pid = None;
    for n in 0..TERMINALS {
        let spawned = client
            .call(
                "terminal.spawn",
                json!({
                    "argv": common::sleep(600),
                    "cwd": "/",
                    "env": {"PATH": common::path()},
                    "session_env": {},
                    "cols": 80,
                    "rows": 24,
                    "labels": {"run": format!("run_{n}"), "agent": "test"},
                    "env_base": "host",
                }),
            )
            .unwrap_or_else(|e| panic!("terminal {}: {e:?}", n + 1));
        if first_pid.is_none() {
            first_pid = spawned["pid"].as_u64();
        }
    }
    let list = client.call("terminal.list", json!({})).unwrap();
    let live = list
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t.get("ended_at").is_none())
        .count();
    assert_eq!(live, TERMINALS, "{list}");

    let after = holds(host_pid);
    let per = |a: usize, b: usize| (b as f64 - a as f64) / TERMINALS as f64;
    eprintln!(
        "host: {} descriptors (+{:.1} each), {} threads (+{:.1} each), {} KiB (+{:.0} KiB each)",
        after.0,
        per(before.0, after.0),
        after.1,
        per(before.1, after.1),
        after.2,
        per(before.2 as usize, after.2 as usize),
    );
    if let Some(pid) = first_pid {
        let holder = holds(parent(pid));
        eprintln!(
            "holder: {} descriptors, {} threads, {} KiB",
            holder.0, holder.1, holder.2
        );
    }
}

#[test]
fn the_limit_follows_the_open_files_the_host_may_have() {
    use pinrail_host::server::terminal_limit;
    // macOS and Linux with a hard limit of 8,192 or more: the host raises
    // its own limit to 8,192
    assert_eq!(terminal_limit(8_192), 1_008);
    // a hard limit of 4,096, as some Linux systems keep
    assert_eq!(terminal_limit(4_096), 496);
    // a limit too small for the descriptors the host keeps for itself
    assert_eq!(terminal_limit(100), 0);
}
