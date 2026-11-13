//! The core starts the host so that it outlives the app: from a versioned
//! copy, outside the core's process tree, with the core's environment, and
//! adopts the one already running rather than starting a second.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

use pinrail_host::client::Client;
use pinrail_host::launch::{Launcher, Method};
use pinrail_host::registration::Registration;
use serde_json::json;

/// The started host, stopped when dropped.
struct Started(Registration);

impl Drop for Started {
    fn drop(&mut self) {
        if let Ok(mut client) = Client::connect(&self.0) {
            let _ = client.call("host.stop", json!({}));
        }
        // in case the call failed, quietly: a host that stopped is gone
        let _ = Command::new("kill")
            .arg(self.0.pid.to_string())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

fn launcher(data_dir: &Path, method: Method) -> Launcher {
    let mut env: Vec<(String, String)> = std::env::vars().filter(|(k, _)| k != "USER").collect();
    env.push(("USER".to_string(), "launch-test-user".to_string()));
    Launcher {
        bundled: PathBuf::from(env!("CARGO_BIN_EXE_pinrail-host")),
        data_dir: data_dir.to_path_buf(),
        env,
        methods: vec![method],
    }
}

fn parent_of(pid: u32) -> u32 {
    let output = Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap()
}

/// Starts a host with `method` and checks what every method must give.
fn starts_outside_the_core(method: Method) {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let logged = std::cell::RefCell::new(Vec::new());
    let log = |line: &str| logged.borrow_mut().push(line.to_string());
    let (registration, how) = launcher(&data, method).ensure(&log).unwrap();
    let started = Started(registration.clone());
    assert_eq!(how, Some(method), "{:?}", logged.borrow());

    // it runs from the versioned copy, not the bundled binary
    let copies: Vec<_> = std::fs::read_dir(data.join("host/bin"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(copies.len(), 1);
    // and is no child of the core
    assert_ne!(parent_of(registration.pid), std::process::id());
    // with the environment it was given
    let client = Client::connect(&registration).unwrap();
    assert_eq!(client.hello["user"], "launch-test-user");
    drop(client);

    // the next core adopts it
    let (again, how) = launcher(&data, method).ensure(&log).unwrap();
    assert_eq!(how, None);
    assert_eq!(again, registration);
    drop(started);
}

#[test]
fn a_detached_host_leaves_the_core_s_process_tree() {
    starts_outside_the_core(Method::Detached);
}

#[cfg(target_os = "macos")]
#[test]
fn a_host_started_by_launchd_leaves_the_core_s_process_tree() {
    // a session without a GUI domain, such as some CI runners, has no
    // launchd route; the core then falls back to a detached host
    let domain = format!("gui/{}", user_id());
    let has_domain = Command::new("/bin/launchctl")
        .args(["print", &domain])
        .output()
        .is_ok_and(|o| o.status.success());
    if !has_domain {
        eprintln!("skipped: no launchd domain {domain}");
        return;
    }
    starts_outside_the_core(Method::Launchd);
    // the one-shot job is gone, so launchd never starts it again
    let jobs = Command::new("/bin/launchctl").arg("list").output().unwrap();
    assert!(!String::from_utf8_lossy(&jobs.stdout).contains("dev.pinrail.host."));
}

#[cfg(target_os = "macos")]
fn user_id() -> String {
    let output = Command::new("id").arg("-u").output().unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
