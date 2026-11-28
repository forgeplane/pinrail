//! Agents start from the person's login environment, read from their login
//! shell without being misled or held up by what its rc files do. Windows
//! has no login shell: there it is what a new terminal would get, read
//! afresh from the person's and the system's settings.

mod common;

use std::time::{Duration, Instant};

use pinrail_host::client::Client;
#[cfg(unix)]
use pinrail_host::probe;
use serde_json::{Value, json};

use common::folders;

#[cfg(unix)]
const PROFILE: &str = r#"
echo "noise from the profile"
printf 'FAKE=from-noise\0'
export PROBED_VAR=from-login
export PROBE_SAW="$PINRAIL_ENV_PROBE"
export PATH="$HOME/bin:$PATH"
# a job that keeps the probe's output open
sleep 3 &
"#;

/// What `script` printed in a terminal started from `env_base`.
fn screen_after_exit(client: &mut Client, env_base: &str, cwd: &str, script: &str) -> String {
    let spawned = client
        .call(
            "terminal.spawn",
            json!({
                "argv": common::shell(script),
                "cwd": cwd,
                "cols": 200,
                "rows": 24,
                "env_base": env_base,
            }),
        )
        .unwrap();
    let id = spawned["terminal"].as_str().unwrap().to_string();
    let deadline = Instant::now() + common::within(10);
    loop {
        let list = client.call("terminal.list", json!({})).unwrap();
        let ended = list
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["terminal"] == id.as_str() && t.get("ended_at").is_some());
        if ended {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let text = client
        .call("terminal.text", json!({"terminal": id, "lines": 0}))
        .unwrap();
    text["text"].as_str().unwrap().to_string()
}

#[cfg(unix)]
#[test]
fn an_agent_starts_from_the_login_environment() {
    let folders = folders();
    std::fs::write(folders.home.join(".profile"), PROFILE).unwrap();
    let host = common::serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    let cwd = folders.runtime.to_str().unwrap().to_string();

    let started = Instant::now();
    let probed: Value = client.call("env.probe", json!({"refresh": true})).unwrap();
    assert_eq!(probed["ok"], true, "{probed}");
    assert!(probed["path_entries"].as_u64().unwrap() >= 2);
    // not held up by the job that keeps the output open
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );

    let script = "echo \"var=$PROBED_VAR saw=$PROBE_SAW fake=$FAKE\"; echo \"path=$PATH\"";
    let screen = screen_after_exit(&mut client, "login_shell", &cwd, script);
    // the rc file's variable, which saw the probe's marker, and not what
    // its noise looked like
    assert!(screen.contains("var=from-login saw=1 fake="), "{screen}");
    assert!(!screen.contains("fake=from-noise"));
    let home_bin = format!("{}/bin", folders.home.display());
    assert!(screen.contains(&format!("path={home_bin}:")), "{screen}");

    // the host's own environment, when asked for
    let screen = screen_after_exit(&mut client, "host", &cwd, script);
    assert!(screen.contains("var= saw="), "{screen}");
}

/// A host that has run since before an installer changed the `PATH` still
/// starts agents with the `PATH` a new terminal gets, and with what only
/// the host's own environment has.
#[cfg(windows)]
#[test]
fn an_agent_starts_from_the_login_environment() {
    let folders = folders();
    let mut serve = common::command(&folders, &["serve"]);
    serve
        .env("PATH", r"C:\a-path-from-before")
        .env("PINRAIL_ONLY_THE_HOST", "kept");
    let host = common::started(serve, &folders);
    let mut client = Client::connect(&host.registration).unwrap();
    let cwd = folders.runtime.to_str().unwrap().to_string();

    let probed: Value = client.call("env.probe", json!({"refresh": true})).unwrap();
    assert_eq!(probed["ok"], true, "{probed}");
    assert!(probed["path_entries"].as_u64().unwrap() >= 2);

    // whether the PATH is the host's old one, and whether it finds what
    // Windows' own has
    let script = "case \"$PATH\" in *a-path-from-before*) echo path=old;; *) echo path=new;; esac; \
                  command -v cmd > /dev/null && echo cmd=found; \
                  echo \"var=$PINRAIL_ONLY_THE_HOST\"";
    let screen = screen_after_exit(&mut client, "login_shell", &cwd, script);
    assert!(screen.contains("path=new"), "{screen}");
    assert!(screen.contains("cmd=found"), "{screen}");
    assert!(screen.contains("var=kept"), "{screen}");

    // the host's own environment, when asked for
    let screen = screen_after_exit(&mut client, "host", &cwd, script);
    assert!(screen.contains("path=old"), "{screen}");
    assert!(!screen.contains("cmd=found"), "{screen}");
    assert!(screen.contains("var=kept"), "{screen}");
}

// Windows runs no shell to read the environment from, so there is none
// that can be slow, loud or missing: the three tests below are Unix's.

#[cfg(unix)]
#[test]
fn a_shell_that_does_not_answer_in_time_falls_back_to_the_host_s_environment() {
    let dir = tempfile::tempdir().unwrap();
    let shell = dir.path().join("slow-shell");
    std::fs::write(&shell, "#!/bin/sh\nsleep 10\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();

    let started = Instant::now();
    let outcome = probe::probe(Some(shell.to_str().unwrap()), Duration::from_millis(500));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(outcome.failure.unwrap().contains("took longer"));
    let path = &outcome.vars.iter().find(|(k, _)| k == "PATH").unwrap().1;
    assert!(path.contains("/opt/homebrew/bin"));
}

#[cfg(unix)]
#[test]
fn a_shell_that_prints_without_end_is_cut_off() {
    let dir = tempfile::tempdir().unwrap();
    let shell = dir.path().join("loud-shell");
    std::fs::write(&shell, "#!/bin/sh\nyes x\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
    // the time allowed is not what is tested here, so it is long: macOS
    // checks a script the first time it runs, and with other tests running
    // that alone has taken close to three seconds before the first byte
    let outcome = probe::probe(Some(shell.to_str().unwrap()), Duration::from_secs(60));
    assert!(outcome.failure.unwrap().contains("1 MiB"));
}

#[cfg(unix)]
#[test]
fn a_missing_shell_falls_back() {
    let outcome = probe::probe(Some("/no/such/shell"), Duration::from_secs(1));
    assert!(outcome.failure.unwrap().contains("did not start"));
}
