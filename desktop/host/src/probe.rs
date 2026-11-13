//! The login environment probe: the variables a person's login shell sets,
//! such as the `PATH` their version managers add to, so that an agent
//! started from the app finds the commands it would find in a terminal.

use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::pty;

/// How long the shell may take.
pub const TIMEOUT: Duration = Duration::from_secs(5);
/// How long a probed environment is reused.
const KEEP: Duration = Duration::from_secs(60);
/// The most output read from the shell.
const MAX_OUTPUT: usize = 1024 * 1024;

pub type Vars = Vec<(String, String)>;

/// What a probe found.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// The environment to start agents from: the probed one, or the
    /// fallback when the probe failed.
    pub vars: Vars,
    /// Why the probe failed, when it did.
    pub failure: Option<String>,
    pub elapsed: Duration,
}

impl Outcome {
    pub fn path_entries(&self) -> usize {
        self.vars
            .iter()
            .find(|(k, _)| k == "PATH")
            .map(|(_, v)| v.split(':').filter(|p| !p.is_empty()).count())
            .unwrap_or(0)
    }
}

/// The probe's result, kept for a minute.
#[derive(Default)]
pub struct Probe {
    kept: Mutex<Option<(Instant, Outcome)>>,
}

impl Probe {
    /// The login environment, probed again when `refresh` is set or the
    /// kept one is older than a minute. Blocks for up to [`TIMEOUT`].
    pub fn get(&self, refresh: bool) -> Outcome {
        let mut kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        if !refresh
            && let Some((at, outcome)) = kept.as_ref()
            && at.elapsed() < KEEP
        {
            return outcome.clone();
        }
        let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty());
        let outcome = probe(shell.as_deref(), TIMEOUT);
        *kept = Some((Instant::now(), outcome.clone()));
        outcome
    }
}

/// Runs the person's login shell once and reads its environment.
pub fn probe(shell: Option<&str>, timeout: Duration) -> Outcome {
    let started = Instant::now();
    let host: Vars = std::env::vars().collect();
    match run(shell, timeout) {
        Ok(vars) => Outcome {
            vars,
            failure: None,
            elapsed: started.elapsed(),
        },
        Err(reason) => Outcome {
            vars: fallback(host),
            failure: Some(reason),
            elapsed: started.elapsed(),
        },
    }
}

/// The host's environment with the usual folders of installed commands
/// added to `PATH`.
pub fn fallback(mut vars: Vars) -> Vars {
    let home = vars
        .iter()
        .find(|(k, _)| k == "HOME")
        .map(|(_, v)| PathBuf::from(v));
    let mut extra = vec![
        "/opt/homebrew/bin".to_string(),
        "/usr/local/bin".to_string(),
    ];
    if let Some(home) = home {
        extra.push(home.join(".local/bin").to_string_lossy().into_owned());
    }
    match vars.iter_mut().find(|(k, _)| k == "PATH") {
        Some((_, path)) => {
            for dir in extra {
                if !path.split(':').any(|p| p == dir) {
                    path.push(':');
                    path.push_str(&dir);
                }
            }
        }
        None => vars.push((
            "PATH".to_string(),
            format!("/usr/bin:/bin:{}", extra.join(":")),
        )),
    }
    vars
}

fn default_shell() -> &'static str {
    if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/bash"
    }
}

/// Runs `$SHELL -l -i -c` with a command that prints the environment
/// between two random markers, and parses only what lies between them. The
/// shell runs in its own process group, which is killed as soon as the end
/// marker arrives or the time runs out, without waiting for end of file:
/// a job an rc file starts can hold the pipe open.
fn run(shell: Option<&str>, timeout: Duration) -> Result<Vars, String> {
    let shell = shell.unwrap_or(default_shell());
    let start = format!("PINRAIL-ENV-{}", crate::random_hex(8));
    let end = format!("PINRAIL-ENV-{}", crate::random_hex(8));
    let script = format!(
        "/usr/bin/printf '\\000%s\\000' {start}; /usr/bin/env -0; /usr/bin/printf '\\000%s\\000' {end}"
    );
    let name = Path::new(shell)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut command = Command::new(shell);
    // csh and tcsh take -l only as their sole option, so they start as
    // login shells with a leading - in their name
    if name == "csh" || name == "tcsh" {
        command.arg0(format!("-{name}")).args(["-i", "-c", &script]);
    } else {
        command.args(["-l", "-i", "-c", &script]);
    }
    let mut child = command
        .env("PINRAIL_ENV_PROBE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("{shell} did not start: {e}"))?;
    let group = child.id();
    let mut stdout = child.stdout.take().expect("piped");
    let deadline = Instant::now() + timeout;
    let (start_marker, end_marker) = (marker(&start), marker(&end));
    let mut output = Vec::new();
    let found = loop {
        if let Some(found) = between(&output, &start_marker, &end_marker) {
            break Ok(found.to_vec());
        }
        if output.len() > MAX_OUTPUT {
            break Err("the shell printed more than 1 MiB".to_string());
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break Err(format!(
                "{shell} took longer than {} seconds",
                timeout.as_secs()
            ));
        }
        match pty::readable_within(&stdout, left) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(e) => break Err(e.to_string()),
        }
        let mut buf = [0u8; 16 * 1024];
        match stdout.read(&mut buf) {
            Ok(0) => break Err(format!("{shell} ended without printing its environment")),
            Ok(n) => output.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => break Err(e.to_string()),
        }
    };
    let _ = pty::signal_group(group, libc::SIGKILL);
    drop(stdout);
    // reaped in the background, so a shell that is slow to die does not
    // hold the caller
    std::thread::spawn(move || child.wait());
    found.map(|bytes| parse(&bytes))
}

fn marker(name: &str) -> Vec<u8> {
    let mut marker = vec![0u8];
    marker.extend(name.as_bytes());
    marker.push(0);
    marker
}

/// The bytes between the first `start` and the next `end`, once both
/// arrived.
fn between<'a>(output: &'a [u8], start: &[u8], end: &[u8]) -> Option<&'a [u8]> {
    let from = find(output, start)? + start.len();
    let to = from + find(&output[from..], end)?;
    Some(&output[from..to])
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// `env -0`'s output: `NAME=value` entries separated by NUL bytes.
fn parse(bytes: &[u8]) -> Vars {
    bytes
        .split(|&b| b == 0)
        .filter_map(|entry| {
            let entry = std::str::from_utf8(entry).ok()?;
            let (name, value) = entry.split_once('=')?;
            (!name.is_empty()).then(|| (name.to_string(), value.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_what_lies_between_the_markers_is_read() {
        let mut output = b"rc file says hello\0".to_vec();
        output.extend(marker("S"));
        output.extend(b"PATH=/a:/b\0MULTI=one\ntwo\0BROKEN\0");
        output.extend(marker("E"));
        output.extend(b"FAKE=after\0");
        let vars = parse(between(&output, &marker("S"), &marker("E")).unwrap());
        assert_eq!(
            vars,
            [
                ("PATH".to_string(), "/a:/b".to_string()),
                ("MULTI".to_string(), "one\ntwo".to_string())
            ]
        );
        assert!(between(b"\0S\0PATH=/a", &marker("S"), &marker("E")).is_none());
    }

    #[test]
    fn the_fallback_adds_the_usual_folders_to_path() {
        let vars = fallback(vec![
            ("HOME".to_string(), "/Users/p".to_string()),
            ("PATH".to_string(), "/usr/bin:/usr/local/bin".to_string()),
        ]);
        let path = &vars.iter().find(|(k, _)| k == "PATH").unwrap().1;
        assert_eq!(
            path,
            "/usr/bin:/usr/local/bin:/opt/homebrew/bin:/Users/p/.local/bin"
        );
    }
}
