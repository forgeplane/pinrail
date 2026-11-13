//! Starting the host so that it outlives the app: from a versioned copy of
//! the bundled binary, through launchd on macOS or `systemd-run` on Linux,
//! and otherwise as a process detached into its own session. The core
//! calls [`Launcher::ensure`]; the host's side of the launchd handshake is
//! [`receive_env`] and [`report_detached`].

use std::fs::{self, DirBuilder};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::client::Client;
use crate::paths::Paths;
use crate::registration::Registration;

/// How long a host has to register and answer after it is launched. A
/// busy machine, or one that has just installed an update, can need more
/// than 5 seconds.
pub const START_WITHIN: Duration = Duration::from_secs(15);

/// How the host was started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// A one-shot launchd job in the user's GUI domain, on macOS.
    Launchd,
    /// A transient unit of the user's systemd manager, on Linux.
    SystemdRun,
    /// A process detached into its own session, which may end with the
    /// app on macOS, or with its scope on Linux.
    Detached,
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// No host answered within [`START_WITHIN`].
    NotStarted,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(error) => write!(f, "{error}"),
            Error::NotStarted => write!(
                f,
                "the terminal host did not start within {} seconds",
                START_WITHIN.as_secs()
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::Io(error)
    }
}

/// What the core needs to start a host.
pub struct Launcher {
    /// The `pinrail-host` binary the app bundles.
    pub bundled: PathBuf,
    pub data_dir: PathBuf,
    /// The environment the host runs with, which its agents start from.
    pub env: Vec<(String, String)>,
    /// The methods to try, in order; [`Launcher::methods`] gives the
    /// system's.
    pub methods: Vec<Method>,
}

impl Launcher {
    /// The launch methods of this system, best first.
    pub fn methods() -> Vec<Method> {
        if cfg!(target_os = "macos") {
            vec![Method::Launchd, Method::Detached]
        } else {
            vec![Method::SystemdRun, Method::Detached]
        }
    }

    /// The running host of this boot, or a new one: its registration, and
    /// how it was started when it is new.
    pub fn ensure(&self, log: &dyn Fn(&str)) -> Result<(Registration, Option<Method>), Error> {
        let paths = Paths::new(&self.data_dir);
        if let Some(registration) = answering(&paths) {
            return Ok((registration, None));
        }
        paths.create()?;
        let copy = install(&self.bundled, &self.data_dir)?;
        for &method in &self.methods {
            let launched = match method {
                Method::Launchd => launchd(&copy, &paths, &self.data_dir, &self.env),
                Method::SystemdRun => systemd_run(&copy, &paths, &self.data_dir, &self.env),
                Method::Detached => detached(&copy, &self.data_dir, &self.env),
            };
            match launched {
                Ok(()) => {
                    if method == Method::Detached && self.methods.len() > 1 {
                        log("the terminal host was started detached and may end with the app");
                    }
                    let registration = wait_for_host(&paths, START_WITHIN)?;
                    return Ok((registration, Some(method)));
                }
                Err(error) => log(&format!(
                    "starting the terminal host with {method:?}: {error}"
                )),
            }
        }
        Err(Error::NotStarted)
    }
}

/// The registered host, if it is of this boot and answers `hello`.
fn answering(paths: &Paths) -> Option<Registration> {
    let registration = Registration::read(&paths.registration())?;
    (registration.may_be_live() && Client::connect(&registration).is_ok()).then_some(registration)
}

/// Waits for a host to register and answer.
pub fn wait_for_host(paths: &Paths, within: Duration) -> Result<Registration, Error> {
    let deadline = Instant::now() + within;
    loop {
        if let Some(registration) = answering(paths) {
            return Ok(registration);
        }
        if Instant::now() >= deadline {
            return Err(Error::NotStarted);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Copies the bundled binary to `<data dir>/host/bin/<version>-<sha8>/`,
/// unless that copy exists, and returns the copy. A running host stays
/// independent of the app's bundle, which an update replaces and an
/// AppImage unmounts when the app quits.
pub fn install(bundled: &Path, data_dir: &Path) -> io::Result<PathBuf> {
    let bytes = fs::read(bundled)?;
    let digest = Sha256::digest(&bytes);
    let sha8: String = digest[..4].iter().map(|b| format!("{b:02x}")).collect();
    let bin = data_dir.join("host/bin");
    let folder = bin.join(format!("{}-{sha8}", env!("CARGO_PKG_VERSION")));
    let copy = folder.join("pinrail-host");
    if copy.is_file() {
        return Ok(copy);
    }
    DirBuilder::new().recursive(true).mode(0o700).create(&bin)?;
    // written beside, then renamed into place, so a copy is never half
    let staging = bin.join(format!(".{sha8}.{}", crate::random_hex(4)));
    DirBuilder::new().mode(0o700).create(&staging)?;
    let staged = staging.join("pinrail-host");
    fs::write(&staged, &bytes)?;
    fs::set_permissions(&staged, fs::Permissions::from_mode(0o700))?;
    match fs::rename(&staging, &folder) {
        Ok(()) => Ok(copy),
        // another core copied it meanwhile
        Err(_) if copy.is_file() => {
            let _ = fs::remove_dir_all(&staging);
            Ok(copy)
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

/// Starts `serve --daemonize` as a child, which forks into its own session
/// and exits, so the host is no longer the app's child.
fn detached(copy: &Path, data_dir: &Path, env: &[(String, String)]) -> io::Result<()> {
    let status = Command::new(copy)
        .args(["serve", "--daemonize", "--data-dir"])
        .arg(data_dir)
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "pinrail-host exited with {status}"
        )))
    }
}

/// The launchd job's label.
const LABEL_PREFIX: &str = "dev.pinrail.host";

/// Starts the host as a one-shot launchd job in the user's GUI domain, so
/// that it has its own coalition and survives the app. The job's process
/// receives the environment over a socket, so that no value is written
/// into the job description, forks, reports that it detached, and exits;
/// the job is then removed, so that launchd never starts it again.
fn launchd(
    copy: &Path,
    paths: &Paths,
    data_dir: &Path,
    env: &[(String, String)],
) -> io::Result<()> {
    let id = crate::random_hex(4);
    let label = format!("{LABEL_PREFIX}.{id}");
    let folder = paths.socket_dir.join(format!("launch.{id}"));
    DirBuilder::new().mode(0o700).create(&folder)?;
    let result = launchd_in(copy, &folder, &label, data_dir, env);
    let _ = fs::remove_dir_all(&folder);
    result
}

fn launchd_in(
    copy: &Path,
    folder: &Path,
    label: &str,
    data_dir: &Path,
    env: &[(String, String)],
) -> io::Result<()> {
    let socket = folder.join("launch.sock");
    let listener = UnixListener::bind(&socket)?;
    let job = folder.join(format!("{label}.plist"));
    fs::write(&job, job_description(label, copy, data_dir, &socket))?;
    let domain = format!("gui/{}", crate::sys::uid());
    launchctl(&["bootstrap", &domain, &job.to_string_lossy()])?;
    let started = launchctl(&["kickstart", &format!("{domain}/{label}")])
        .and_then(|()| hand_over(&listener, env));
    // removed whatever happened, so that launchd never starts it again
    let _ = launchctl(&["bootout", &format!("{domain}/{label}")]);
    started
}

fn launchctl(args: &[&str]) -> io::Result<()> {
    let output = Command::new("/bin/launchctl")
        .args(args)
        .stdin(Stdio::null())
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "launchctl {} failed: {}",
            args[0],
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// A launchd job that runs once, when kickstarted, and whose process group
/// launchd leaves alone when the job's process exits.
fn job_description(label: &str, copy: &Path, data_dir: &Path, socket: &Path) -> String {
    let arguments = [
        copy.to_string_lossy(),
        "serve".into(),
        "--daemonize".into(),
        "--handshake".into(),
        socket.to_string_lossy(),
        "--data-dir".into(),
        data_dir.to_string_lossy(),
    ];
    let arguments: String = arguments
        .iter()
        .map(|a| format!("    <string>{}</string>\n", xml_escape(a)))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
{arguments}  </array>
  <key>RunAtLoad</key>
  <false/>
  <key>KeepAlive</key>
  <false/>
  <key>AbandonProcessGroup</key>
  <true/>
  <key>ProcessType</key>
  <string>Interactive</string>
</dict>
</plist>
"#
    )
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The core's side of the handshake: sends the environment to the
/// launched process and waits until it reports that it detached.
fn hand_over(listener: &UnixListener, env: &[(String, String)]) -> io::Result<()> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + START_WITHIN;
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "the launched host did not connect",
                    ));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(error),
        }
    };
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(START_WITHIN))?;
    let pairs: Vec<Value> = env.iter().map(|(k, v)| json!([k, v])).collect();
    writeln!(stream, "{}", json!({ "env": pairs }))?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let reply: Value = serde_json::from_str(&line).map_err(io::Error::other)?;
    if reply.get("detached").and_then(Value::as_u64).is_some() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "the launched host answered {line:?}"
        )))
    }
}

/// The host's side of the handshake: connects to the core's socket and
/// takes the environment it sends as its own. Call it while the process
/// has one thread.
pub fn receive_env(socket: &Path) -> io::Result<UnixStream> {
    let stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(START_WITHIN))?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let message: Value = serde_json::from_str(&line).map_err(io::Error::other)?;
    let vars: Vec<(String, String)> = message
        .get("env")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|pair| {
            Some((
                pair.get(0)?.as_str()?.to_string(),
                pair.get(1)?.as_str()?.to_string(),
            ))
        })
        .collect();
    crate::sys::replace_env(&vars);
    Ok(stream)
}

/// Leaves the caller's process tree, as `serve --daemonize` does: forks,
/// the parent exits, and the child goes on in a new session. Call it while
/// the process has one thread.
pub fn daemonize() -> io::Result<()> {
    crate::sys::daemonize()
}

/// Tells the core that this process detached, from the process that
/// detached.
pub fn report_detached(mut stream: UnixStream) -> io::Result<()> {
    writeln!(stream, "{}", json!({ "detached": std::process::id() }))
}

/// Starts the host as a transient unit of the user's systemd manager, so
/// that it lives outside the app's scope. The environment goes in a 0600
/// file that `EnvironmentFile` names, never on the command line.
fn systemd_run(
    copy: &Path,
    paths: &Paths,
    data_dir: &Path,
    env: &[(String, String)],
) -> io::Result<()> {
    let version = systemd_version()?;
    let id = crate::random_hex(4);
    let env_file = paths.socket_dir.join(format!("host.{id}.env"));
    write_private(&env_file, environment_file(env).as_bytes())?;
    let args = systemd_run_args(version, &id, &env_file, copy, data_dir);
    let output = Command::new("systemd-run")
        .args(&args)
        .stdin(Stdio::null())
        .output();
    // systemd reads the file as the unit starts; a host that has
    // registered no longer needs it, and one that failed never will
    let result = match output {
        Ok(output) if output.status.success() => wait_for_host(paths, START_WITHIN)
            .map(|_| ())
            .map_err(|e| io::Error::other(e.to_string())),
        Ok(output) => Err(io::Error::other(format!(
            "systemd-run failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))),
        Err(error) => Err(error),
    };
    let _ = fs::remove_file(&env_file);
    result
}

/// The version of systemd that `systemd-run --version` reports, which
/// must be 239 or later.
fn systemd_version() -> io::Result<u32> {
    let output = Command::new("systemd-run")
        .arg("--version")
        .stdin(Stdio::null())
        .output()?;
    parse_systemd_version(&String::from_utf8_lossy(&output.stdout))
        .filter(|&v| v >= 239)
        .ok_or_else(|| io::Error::other("systemd-run is missing or older than 239"))
}

fn parse_systemd_version(text: &str) -> Option<u32> {
    text.split_whitespace().nth(1)?.parse().ok()
}

/// `systemd-run`'s arguments for a version: `--service-type=exec` from
/// 240, `OOMPolicy` from 243.
fn systemd_run_args(
    version: u32,
    id: &str,
    env_file: &Path,
    copy: &Path,
    data_dir: &Path,
) -> Vec<String> {
    let mut args = vec![
        "--user".to_string(),
        "--quiet".to_string(),
        "--collect".to_string(),
        format!(
            "--service-type={}",
            if version >= 240 { "exec" } else { "simple" }
        ),
        format!("--unit=pinrail-host-{id}"),
        "-p".to_string(),
        format!("EnvironmentFile={}", env_file.display()),
        "-p".to_string(),
        "KillMode=process".to_string(),
    ];
    if version >= 243 {
        args.extend(["-p".to_string(), "OOMPolicy=continue".to_string()]);
    }
    args.extend([
        copy.to_string_lossy().into_owned(),
        "serve".to_string(),
        "--data-dir".to_string(),
        data_dir.to_string_lossy().into_owned(),
    ]);
    args
}

/// An `EnvironmentFile`: a quoted assignment per line. A variable whose
/// name is not a shell name, or whose value spans lines, is left out.
fn environment_file(env: &[(String, String)]) -> String {
    env.iter()
        .filter(|(name, value)| {
            !name.is_empty()
                && !name.starts_with(|c: char| c.is_ascii_digit())
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !value.contains(['\n', '\r', '\0'])
        })
        .map(|(name, value)| {
            let quoted = value.replace('\\', "\\\\").replace('"', "\\\"");
            format!("{name}=\"{quoted}\"\n")
        })
        .collect()
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_copy_is_made_once_per_version_and_content() {
        let dir = tempfile::tempdir().unwrap();
        let bundled = dir.path().join("pinrail-host");
        fs::write(&bundled, b"binary one").unwrap();
        let data = dir.path().join("data");
        let first = install(&bundled, &data).unwrap();
        assert_eq!(first.file_name().unwrap(), "pinrail-host");
        let folder = first
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        assert!(folder.starts_with(&format!("{}-", env!("CARGO_PKG_VERSION"))));
        assert_eq!(fs::read(&first).unwrap(), b"binary one");
        let mode = fs::metadata(&first).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        assert_eq!(install(&bundled, &data).unwrap(), first);
        // a new build goes to a new folder, and the old copy stays
        fs::write(&bundled, b"binary two").unwrap();
        let second = install(&bundled, &data).unwrap();
        assert_ne!(second, first);
        assert!(first.is_file());
        // nothing half-made is left behind
        assert_eq!(fs::read_dir(data.join("host/bin")).unwrap().count(), 2);
    }

    #[test]
    fn systemd_run_s_arguments_follow_its_version() {
        let args = |version| {
            systemd_run_args(
                version,
                "ab12",
                Path::new("/run/user/1000/p/host.ab12.env"),
                Path::new("/data/host/bin/0.6.0-1234abcd/pinrail-host"),
                Path::new("/data"),
            )
        };
        let current = args(255);
        assert_eq!(
            current,
            [
                "--user",
                "--quiet",
                "--collect",
                "--service-type=exec",
                "--unit=pinrail-host-ab12",
                "-p",
                "EnvironmentFile=/run/user/1000/p/host.ab12.env",
                "-p",
                "KillMode=process",
                "-p",
                "OOMPolicy=continue",
                "/data/host/bin/0.6.0-1234abcd/pinrail-host",
                "serve",
                "--data-dir",
                "/data",
            ]
        );
        assert!(args(242).iter().all(|a| !a.starts_with("OOMPolicy")));
        assert!(args(242).contains(&"--service-type=exec".to_string()));
        assert!(args(239).contains(&"--service-type=simple".to_string()));
    }

    #[test]
    fn systemd_s_version_is_read_from_its_first_line() {
        assert_eq!(
            parse_systemd_version("systemd 255 (255.4-1ubuntu8)\n+PAM +AUDIT"),
            Some(255)
        );
        assert_eq!(parse_systemd_version(""), None);
    }

    #[test]
    fn the_environment_file_quotes_values_and_skips_what_it_cannot_hold() {
        let env = [
            ("PATH", "/usr/bin:/bin"),
            ("QUOTED", r#"say "hi" \ $HOME"#),
            ("MULTI", "two\nlines"),
            ("BASH_FUNC_x%%", "() { :; }"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        assert_eq!(
            environment_file(&env),
            "PATH=\"/usr/bin:/bin\"\nQUOTED=\"say \\\"hi\\\" \\\\ $HOME\"\n"
        );
    }

    #[test]
    fn the_job_description_runs_once_and_abandons_its_process_group() {
        let job = job_description(
            "dev.pinrail.host.ab12",
            Path::new("/Users/p/Library/a&b/pinrail-host"),
            Path::new("/Users/p/data"),
            Path::new("/tmp/s/launch.sock"),
        );
        assert!(job.contains("<string>/Users/p/Library/a&amp;b/pinrail-host</string>"));
        assert!(job.contains("<key>RunAtLoad</key>\n  <false/>"));
        assert!(job.contains("<key>KeepAlive</key>\n  <false/>"));
        assert!(job.contains("<key>AbandonProcessGroup</key>\n  <true/>"));
        assert!(job.contains("<string>--handshake</string>"));
        // no environment value is written into it
        assert!(!job.contains("EnvironmentVariables"));
    }
}
