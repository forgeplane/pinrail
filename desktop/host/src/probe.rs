//! The login environment probe: the variables a person's login shell sets,
//! such as the `PATH` their version managers add to, so that an agent
//! started from the app finds the commands it would find in a terminal.
//!
//! Windows has no login shell: there the variables are read afresh from
//! the user's and the system's settings, as a new terminal would get them,
//! which also picks up a `PATH` an installer changed since the host started.

#[cfg(unix)]
use std::io::{self, Read};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[cfg(unix)]
use crate::pty;

/// How long the shell may take.
pub const TIMEOUT: Duration = Duration::from_secs(5);
/// How long a probed environment is reused.
const KEEP: Duration = Duration::from_secs(60);
/// The most output read from the shell.
#[cfg(unix)]
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
        let separator = if cfg!(windows) { ';' } else { ':' };
        self.vars
            .iter()
            .find(|(k, _)| k == "PATH" || cfg!(windows) && k.eq_ignore_ascii_case("PATH"))
            .map(|(_, v)| v.split(separator).filter(|p| !p.is_empty()).count())
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

/// The host's environment as it is: the folders of installed commands are
/// on Windows' own `PATH` already.
#[cfg(windows)]
pub fn fallback(vars: Vars) -> Vars {
    vars
}

/// The variables a new terminal gets: the host's, with the user's and the
/// system's settings read afresh over them.
#[cfg(windows)]
fn run(_shell: Option<&str>, _timeout: Duration) -> Result<Vars, String> {
    let mut env: std::collections::BTreeMap<String, String> = std::env::vars().collect();
    refresh(&mut env).map_err(|e| format!("the settings could not be read: {e}"))?;
    Ok(env.into_iter().collect())
}

/// Puts the user's and the system's variables, as Windows reads them from
/// the registry, over `env`; a variable only this process has stays.
/// This process's token, for reading the user's own environment.
#[cfg(windows)]
#[allow(unsafe_code)]
fn user_token() -> std::io::Result<std::os::windows::io::OwnedHandle> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Security::{TOKEN_DUPLICATE, TOKEN_IMPERSONATE, TOKEN_QUERY};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    let mut token = std::ptr::null_mut();
    // impersonation too, which expanding the user's variables asks for
    // SAFETY: the token is owned below.
    crate::win::check(unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_IMPERSONATE,
            &mut token,
        )
    })?;
    // SAFETY: as above.
    Ok(unsafe { OwnedHandle::from_raw_handle(token.cast()) })
}

/// `text` with the user's `%VARIABLES%` expanded.
#[cfg(windows)]
#[allow(unsafe_code)]
fn expand_for_user(
    token: &std::os::windows::io::OwnedHandle,
    text: &str,
) -> std::io::Result<String> {
    use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsForUserW;
    let source: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let mut expanded = vec![0u16; 32_768];
    // SAFETY: the source is NUL-terminated; the buffer holds its size.
    crate::win::check(unsafe {
        ExpandEnvironmentStringsForUserW(
            crate::win::raw(token),
            source.as_ptr(),
            expanded.as_mut_ptr(),
            expanded.len() as u32,
        )
    })?;
    let len = expanded
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(expanded.len());
    Ok(String::from_utf16_lossy(&expanded[..len]))
}

/// Adapted from herdr-core's crates/herdr-core/src/platform/windows_env.rs.
#[cfg(windows)]
#[allow(unsafe_code)]
fn refresh(env: &mut std::collections::BTreeMap<String, String>) -> std::io::Result<()> {
    use windows_sys::Win32::System::Environment::{
        CreateEnvironmentBlock, DestroyEnvironmentBlock,
    };
    let token = user_token()?;
    let mut block = std::ptr::null_mut();
    // SAFETY: Windows writes a block it allocates, freed below.
    crate::win::check(unsafe { CreateEnvironmentBlock(&mut block, crate::win::raw(&token), 0) })?;
    // SAFETY: the block is `name=value` strings, each ending in a NUL, and
    // an empty one at the end.
    unsafe {
        let mut at = block.cast::<u16>();
        while *at != 0 {
            let len = (0..).take_while(|&i| *at.add(i) != 0).count();
            let entry = String::from_utf16_lossy(std::slice::from_raw_parts(at, len));
            if let Some((key, value)) = entry.split_once('=')
                && !key.is_empty()
            {
                env.retain(|k, _| !k.eq_ignore_ascii_case(key));
                env.insert(key.into(), value.into());
            }
            at = at.add(len + 1);
        }
        DestroyEnvironmentBlock(block);
    }
    // the block can leave references to variables set late, such as
    // ProgramFiles, in PATH
    if let Some((_, path)) = env
        .iter_mut()
        .find(|(key, _)| key.eq_ignore_ascii_case("PATH"))
        && path.contains('%')
    {
        *path = expand_for_user(&token, path)?;
    }
    Ok(())
}

/// The host's environment with the usual folders of installed commands
/// added to `PATH`.
#[cfg(unix)]
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

#[cfg(unix)]
fn default_shell() -> &'static str {
    if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/bash"
    }
}

#[cfg(unix)]
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
    let mut between = Between::new(marker(&start), marker(&end));
    let mut output = Vec::new();
    let found = loop {
        if let Some(found) = between.of(&output) {
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

#[cfg(unix)]
fn marker(name: &str) -> Vec<u8> {
    let mut marker = vec![0u8];
    marker.extend(name.as_bytes());
    marker.push(0);
    marker
}

#[cfg(unix)]
/// Looks for the bytes between the first `start` and the next `end` in
/// output that grows with every read. It remembers how far it looked, so
/// each byte is searched once: searching everything again after every read
/// costs the square of the output, which for a shell that prints a megabyte
/// took seconds of the time the shell is allowed in an unoptimized build.
struct Between {
    start: Vec<u8>,
    end: Vec<u8>,
    /// Where the environment begins, once `start` arrived.
    from: Option<usize>,
    /// How much of the output held neither marker when last looked at.
    searched: usize,
}

#[cfg(unix)]
impl Between {
    fn new(start: Vec<u8>, end: Vec<u8>) -> Self {
        Self {
            start,
            end,
            from: None,
            searched: 0,
        }
    }

    /// The bytes between the markers, once both arrived. `output` is what
    /// the last call saw with more appended.
    fn of<'a>(&mut self, output: &'a [u8]) -> Option<&'a [u8]> {
        let from = match self.from {
            Some(from) => from,
            None => {
                let at = resume(self.searched, &self.start);
                let Some(found) = find(&output[at..], &self.start) else {
                    self.searched = output.len();
                    return None;
                };
                let from = at + found + self.start.len();
                self.from = Some(from);
                self.searched = from;
                from
            }
        };
        let at = resume(self.searched, &self.end).max(from);
        let Some(found) = find(&output[at..], &self.end) else {
            self.searched = output.len();
            return None;
        };
        Some(&output[from..at + found])
    }
}

#[cfg(unix)]
/// Where to look for `needle` again after `searched` bytes held none: a
/// little before their end, since a read can stop in the middle of one.
fn resume(searched: usize, needle: &[u8]) -> usize {
    searched.saturating_sub(needle.len().saturating_sub(1))
}

#[cfg(unix)]
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(unix)]
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

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn the_user_s_settings_come_over_the_host_s_own() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("PINRAIL_ONLY_HERE".to_string(), "kept".to_string());
        env.insert("PATH".to_string(), r"C:\stale".to_string());
        refresh(&mut env).unwrap();
        assert_eq!(env["PINRAIL_ONLY_HERE"], "kept");
        let path: Vec<_> = env
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("path"))
            .collect();
        assert_eq!(path.len(), 1);
        assert!(path[0].1.to_lowercase().contains(r"c:\windows"), "{path:?}");
        assert!(!path[0].1.contains('%'), "{path:?}");
        let outcome = probe(None, TIMEOUT);
        assert!(outcome.failure.is_none());
        assert!(outcome.path_entries() > 1);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn only_what_lies_between_the_markers_is_read() {
        let mut output = b"rc file says hello\0".to_vec();
        output.extend(marker("S"));
        output.extend(b"PATH=/a:/b\0MULTI=one\ntwo\0BROKEN\0");
        output.extend(marker("E"));
        output.extend(b"FAKE=after\0");
        let mut between = Between::new(marker("S"), marker("E"));
        let vars = parse(between.of(&output).unwrap());
        assert_eq!(
            vars,
            [
                ("PATH".to_string(), "/a:/b".to_string()),
                ("MULTI".to_string(), "one\ntwo".to_string())
            ]
        );
        let mut between = Between::new(marker("S"), marker("E"));
        assert!(between.of(b"\0S\0PATH=/a").is_none());
    }

    #[test]
    fn the_markers_are_found_however_the_output_is_cut_into_reads() {
        let mut output = b"noise\0".to_vec();
        output.extend(marker("START"));
        output.extend(b"PATH=/a:/b\0HOME=/Users/p\0");
        output.extend(marker("END"));
        output.extend(b"FAKE=after\0");
        output.extend(marker("END"));
        for step in 1..=output.len() {
            let mut between = Between::new(marker("START"), marker("END"));
            let mut found = None;
            let mut arrived = 0;
            while found.is_none() && arrived < output.len() {
                arrived = (arrived + step).min(output.len());
                found = between.of(&output[..arrived]);
            }
            assert_eq!(
                found,
                Some(&b"PATH=/a:/b\0HOME=/Users/p\0"[..]),
                "{step} bytes at a time"
            );
        }
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

#[cfg(all(test, windows))]
mod token_tests {
    #[test]
    fn the_user_s_variables_expand_with_the_token_the_probe_opens() {
        let token = super::user_token().unwrap();
        // a system variable, and one of the user's own
        let system = std::env::var("SystemRoot").unwrap();
        let profile = std::env::var("USERPROFILE").unwrap();
        assert_eq!(
            super::expand_for_user(&token, r"%SystemRoot%\x;%USERPROFILE%\y").unwrap(),
            format!(r"{system}\x;{profile}\y")
        );
    }
}
