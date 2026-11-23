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
    // in the same test, as another job would show in launchd's list
    a_host_bundle_runs_from_the_executable_in_its_copy();
}

/// Starts a host from `Pinrail Host.app` through launchd, and checks that
/// it runs from the executable inside the bundle's copy.
#[cfg(target_os = "macos")]
fn a_host_bundle_runs_from_the_executable_in_its_copy() {
    let root = tempfile::tempdir().unwrap();
    let contents = root.path().join("Pinrail Host.app/Contents");
    std::fs::create_dir_all(contents.join("MacOS")).unwrap();
    std::fs::write(contents.join("Info.plist"), "<plist><dict/></plist>").unwrap();
    let executable = contents.join("MacOS/pinrail-host");
    std::fs::copy(env!("CARGO_BIN_EXE_pinrail-host"), &executable).unwrap();
    let data = root.path().join("data");
    let mut launcher = launcher(&data, Method::Launchd);
    launcher.bundled = executable;
    let (registration, how) = launcher.ensure(&|_| {}).unwrap();
    let started = Started(registration.clone());
    assert_eq!(how, Some(Method::Launchd));
    let output = Command::new("ps")
        .args(["-o", "comm=", "-p", &registration.pid.to_string()])
        .output()
        .unwrap();
    let running = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let build = std::fs::read_dir(data.join("host/bin"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .next()
        .unwrap();
    let expected = build.join("Pinrail Host.app/Contents/MacOS/pinrail-host");
    assert_eq!(
        std::fs::canonicalize(&running).unwrap(),
        std::fs::canonicalize(&expected).unwrap()
    );
    drop(started);
}

#[cfg(target_os = "macos")]
fn user_id() -> String {
    let output = Command::new("id").arg("-u").output().unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn a_host_of_another_build_is_replaced_and_its_terminals_kept() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let log = |_: &str| {};
    let (old, _) = launcher(&data, Method::Detached).ensure(&log).unwrap();
    let old_host = Started(old.clone());
    let mut client = Client::connect(&old).unwrap();
    let spawned = client
        .call(
            "terminal.spawn",
            json!({"argv": ["sh", "-c", "echo ready; exec sleep 600"],
                   "cwd": root.path(), "cols": 80, "rows": 24,
                   "env": {"PATH": "/usr/bin:/bin"}, "env_base": "host"}),
        )
        .unwrap();
    let terminal = spawned["terminal"].as_str().unwrap().to_string();
    drop(client);

    // the running host says it is another build of the app
    let path = data.join("host/host.json");
    let mut written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(
        written["build"].is_string(),
        "the host records its build: {written}"
    );
    written["build"] = json!("0.0.0-0ldb0ld0");
    std::fs::write(&path, written.to_string()).unwrap();

    let (new, how) = launcher(&data, Method::Detached).ensure(&log).unwrap();
    let _new_host = Started(new.clone());
    assert_eq!(how, Some(Method::Detached));
    assert_ne!(new.pid, old.pid);
    // the old host is gone, and the new one keeps its terminal
    std::mem::forget(old_host);
    let mut client = Client::connect(&new).unwrap();
    let listed = client.call("terminal.list", json!({})).unwrap();
    let kept: Vec<_> = listed
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["terminal"] == terminal.as_str() && t.get("exit").is_none())
        .collect();
    assert_eq!(kept.len(), 1, "{listed}");
    // the old host handed the screen over in the new host's best format
    let log = std::fs::read_to_string(data.join("host/host.log")).unwrap();
    let binary = format!(
        "adopted {terminal} from a hand-over in ghostty-bin;vt-{}",
        pinrail_screen::ghostty::version()
    );
    assert!(log.contains(&binary), "{log}");
    client
        .call(
            "terminal.stop",
            json!({"terminal": terminal, "grace_ms": 0}),
        )
        .unwrap();
}

#[test]
fn the_host_names_the_snapshot_formats_it_reads() {
    let output = Command::new(env!("CARGO_BIN_EXE_pinrail-host"))
        .arg("formats")
        .output()
        .unwrap();
    assert!(output.status.success());
    let formats: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        formats,
        [
            format!("ghostty-bin;vt-{}", pinrail_screen::ghostty::version()),
            "vt".to_string()
        ]
    );
}
