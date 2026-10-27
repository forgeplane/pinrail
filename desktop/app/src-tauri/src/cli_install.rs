//! Putting the bundled CLI on the PATH, which depends on how the app came.
//!
//! - macOS: the bundle carries the CLI as `pinrail-cli` beside `Pinrail`.
//!   Installing links `~/.local/bin/pinrail` to it, so updating the app
//!   updates the CLI and nothing is copied.
//! - A Linux package (.deb, .rpm) installs the CLI as `/usr/bin/pinrail` beside
//!   `pinrail-desktop`; it is on the PATH already and there is nothing to do.
//! - An AppImage carries the same `pinrail`, but mounts itself at a new
//!   temporary path each run, so a link would break: installing copies it to
//!   `~/.local/bin/pinrail` instead, and a copy from an older AppImage can be
//!   replaced by installing again.
//!
//! - Windows: the bundle carries the CLI as `pinrail-cli.exe` beside
//!   `Pinrail.exe`. Installing copies it to `~\.local\bin\pinrail.exe`, since
//!   a link needs a privilege, and puts that folder on the user's PATH.
//!
//! A development build has no bundled CLI.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde::Serialize;

/// The CLI's name in a macOS or Windows bundle, beside `Pinrail`.
const SIDECAR: &str = if cfg!(windows) {
    "pinrail-cli.exe"
} else {
    "pinrail-cli"
};
/// The name a shell runs it by, and its name in the Linux builds.
const COMMAND: &str = if cfg!(windows) {
    "pinrail.exe"
} else {
    "pinrail"
};
/// What `COMMAND --version` starts its answer with.
const PROGRAM: &str = "pinrail";
/// How long a login shell may take to say what its PATH is.
const SHELL_TIMEOUT: Duration = Duration::from_secs(4);
/// How long `pinrail --version` may take when telling an old copy apart.
const VERSION_TIMEOUT: Duration = Duration::from_secs(2);

/// How the bundled CLI reaches the PATH.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// `~/.local/bin/pinrail` links to it
    Link,
    /// a Linux package put it on the PATH; nothing to install
    Package,
    /// `~/.local/bin/pinrail` is a copy of it, since its own path is temporary
    Copy,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Status {
    pub mode: Mode,
    /// the CLI inside the app; none in a development build
    pub bundled: Option<PathBuf>,
    /// where it is installed: the link or the copy, or the package's own file
    pub link: PathBuf,
    /// the link points at the bundled CLI, the copy matches it, or the
    /// package installed it
    pub installed: bool,
    /// a copy from another version of the CLI is there, which installing
    /// replaces
    pub outdated: bool,
    /// something else already has the place: a file, or a link elsewhere
    pub occupied_by: Option<String>,
    /// what `pinrail` runs in a new terminal, when the login shell said
    pub runs: Option<PathBuf>,
    /// whether the install's folder is on a new terminal's PATH; none when
    /// the login shell could not be asked
    pub dir_on_path: Option<bool>,
    /// something that went wrong after the CLI was installed, such as the
    /// PATH not being updated
    pub warning: Option<String>,
    /// the app runs on Windows, where a copy is brought up to date when the
    /// app starts
    pub windows: bool,
}

/// What went wrong after the last install, kept so that the status check that
/// follows still has it. Installing again replaces it.
static LAST_WARNING: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Records the last install's warning, or clears it.
pub fn remember_warning(warning: Option<String>) {
    if let Ok(mut last) = LAST_WARNING.lock() {
        *last = warning;
    }
}

/// `status` with the last install's warning, for as long as the folder is
/// still not on the PATH.
pub fn with_warning(mut status: Status) -> Status {
    if status.dir_on_path != Some(true)
        && let Ok(last) = LAST_WARNING.lock()
    {
        status.warning = last.clone();
    }
    status
}

/// The CLI shipped beside the running app, when there is one.
pub fn bundled() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [SIDECAR, COMMAND]
        .iter()
        .map(|name| dir.join(name))
        // the app's own binary is never the CLI, whatever it is called
        .find(|p| p.is_file() && !same_file(p, &exe))
}

/// How this run of the app installs `bundled`: a copy under an AppImage, the
/// package's own file for a CLI named `pinrail`, and a link otherwise.
pub fn mode(bundled: &Path, appimage: bool) -> Mode {
    if appimage || cfg!(windows) {
        Mode::Copy
    } else if bundled.file_name().is_some_and(|n| n == COMMAND) {
        Mode::Package
    } else {
        Mode::Link
    }
}

/// Whether the app runs from an AppImage, which says so in the environment.
pub fn in_appimage() -> bool {
    std::env::var_os("APPIMAGE").is_some()
}

/// `~/.local/bin/pinrail`, with `.exe` on Windows.
pub fn link_path(home: &Path) -> PathBuf {
    home.join(".local").join("bin").join(COMMAND)
}

/// What is at the install's place, against the bundled CLI, and what a new
/// terminal would run. `shell` is what the login shell reported, if asked.
pub fn status(
    mode: Mode,
    bundled: Option<&Path>,
    link: &Path,
    shell: Option<&ShellView>,
) -> Status {
    let link = match (mode, bundled) {
        (Mode::Package, Some(b)) => b,
        _ => link,
    };
    let mut outdated = false;
    let (installed, occupied_by) = match std::fs::symlink_metadata(link) {
        Err(_) => (false, None),
        Ok(_) if mode == Mode::Package => (true, None),
        Ok(meta) if meta.file_type().is_symlink() => {
            let target = std::fs::read_link(link).ok();
            let ours = mode == Mode::Link
                && match (bundled, target.as_deref()) {
                    (Some(b), Some(t)) => same_file(b, t),
                    _ => false,
                };
            let other = (!ours).then(|| match &target {
                Some(t) => format!("a link to {}", t.display()),
                None => "a link".to_string(),
            });
            (ours, other)
        }
        Ok(meta) if meta.is_dir() => (false, Some("a folder".to_string())),
        Ok(_) if mode == Mode::Copy => {
            if bundled.is_some_and(|b| same_contents(b, link)) {
                (true, None)
            } else if is_pinrail_cli(link) {
                outdated = true;
                (false, None)
            } else {
                (false, Some("a file".to_string()))
            }
        }
        Ok(_) => (false, Some("a file".to_string())),
    };
    let dir = link.parent().map(Path::to_path_buf);
    Status {
        mode,
        bundled: bundled.map(Path::to_path_buf),
        link: link.to_path_buf(),
        installed,
        outdated,
        occupied_by,
        runs: shell.and_then(|s| s.command.clone()),
        warning: None,
        windows: cfg!(windows),
        dir_on_path: shell.map(|s| {
            dir.as_ref()
                .is_some_and(|d| s.path.iter().any(|p| same_file(p, d)))
        }),
    }
}

/// Puts `bundled` at `link` the way `mode` says: a link, replacing a link
/// already there; or a copy, replacing a link or an older copy of the CLI. A
/// file that is not the CLI, or a folder, is left alone and reported. A
/// package has nothing to install.
pub fn install(mode: Mode, bundled: &Path, link: &Path) -> Result<(), String> {
    if mode == Mode::Package {
        return Ok(());
    }
    if let Some(dir) = link.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let replaceable = match std::fs::symlink_metadata(link) {
        Err(_) => false,
        Ok(meta) if meta.file_type().is_symlink() => true,
        Ok(meta) if meta.is_dir() => {
            return Err(format!(
                "{} is a folder; move it aside and install again",
                link.display()
            ));
        }
        Ok(_) if mode == Mode::Copy && is_pinrail_cli(link) => true,
        Ok(_) if mode == Mode::Copy => {
            return Err(format!(
                "{} is a file that is not the pinrail CLI; move it aside and install again",
                link.display()
            ));
        }
        Ok(_) => {
            return Err(format!(
                "{} is a file, not a link; move it aside and install again",
                link.display()
            ));
        }
    };
    match mode {
        Mode::Link => {
            if replaceable {
                std::fs::remove_file(link)
                    .map_err(|e| format!("cannot replace {}: {e}", link.display()))?;
            }
            symlink(bundled, link).map_err(|e| format!("cannot link {}: {e}", link.display()))
        }
        Mode::Copy => copy(bundled, link),
        Mode::Package => Ok(()),
    }
}

/// Brings a copy that [`Mode::Copy`] installed up to the bundled CLI, after
/// an update replaced the app but not the copy (a link follows the app,
/// and a package updates its own file). Only a copy that is some other
/// version of the pinrail CLI is replaced: none is made where the person
/// did not install one, and a file that is not the CLI stays. True when
/// the copy was replaced.
pub fn refresh(bundled: &Path, link: &Path) -> Result<bool, String> {
    if !status(Mode::Copy, Some(bundled), link, None).outdated {
        return Ok(false);
    }
    copy(bundled, link).map(|()| true)
}

/// Copies beside `link` and renames over it, so a terminal never runs half a
/// file, and a link in its place is replaced rather than written through.
fn copy(bundled: &Path, link: &Path) -> Result<(), String> {
    let partial = link.with_file_name(format!(".{PROGRAM}.partial"));
    let result = std::fs::copy(bundled, &partial)
        .and_then(|_| executable(&partial))
        .and_then(|_| replace(&partial, link));
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result.map_err(|e| {
        let busy = if cfg!(windows) && e.kind() == std::io::ErrorKind::PermissionDenied {
            "; close the pinrail commands that are running and try again"
        } else {
            ""
        };
        format!("cannot copy the CLI to {}: {e}{busy}", link.display())
    })
}

/// Renames `partial` over `link`. Windows refuses that while `link` is a
/// running program, but lets a running program be renamed, so the old file
/// moves aside to `.pinrail.old` first, back again if the new one cannot take
/// its place. The old file is deleted at once when nothing runs it, and by
/// the next install otherwise.
#[cfg(windows)]
fn replace(partial: &Path, link: &Path) -> std::io::Result<()> {
    match std::fs::rename(partial, link) {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && link.is_file() => {
            let old = link.with_file_name(format!(".{PROGRAM}.old"));
            let _ = std::fs::remove_file(&old);
            std::fs::rename(link, &old)?;
            if let Err(e) = std::fs::rename(partial, link) {
                let _ = std::fs::rename(&old, link);
                return Err(e);
            }
            let _ = std::fs::remove_file(&old);
            Ok(())
        }
        result => result,
    }
}

#[cfg(not(windows))]
fn replace(partial: &Path, link: &Path) -> std::io::Result<()> {
    std::fs::rename(partial, link)
}

#[cfg(unix)]
fn executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn same_contents(a: &Path, b: &Path) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(ma), Ok(mb)) if ma.len() == mb.len() => {
            matches!((std::fs::read(a), std::fs::read(b)), (Ok(x), Ok(y)) if x == y)
        }
        _ => false,
    }
}

/// Whether `path` is some version of the pinrail CLI: it says `pinrail <version>`
/// when asked. Anything that does not answer in time is not.
fn is_pinrail_cli(path: &Path) -> bool {
    let Ok(mut child) = spawn(
        Command::new(path)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    ) else {
        return false;
    };
    let Some(stdout) = child.stdout.take() else {
        return false;
    };
    let text = read_with_timeout(stdout, VERSION_TIMEOUT, None);
    if text.is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
    text.is_some_and(|t| t.starts_with(&format!("{PROGRAM} ")))
}

/// Starts `command`, and tries again for up to half a second while Linux
/// reports that the program file is busy (ETXTBSY). A file that was
/// written a moment ago stays open for writing in any process that another
/// thread of this one starts at that moment, until that process runs its
/// own program, and Linux refuses to run a file that is open for writing.
fn spawn(command: &mut Command) -> std::io::Result<std::process::Child> {
    let mut tries = 0;
    loop {
        match command.spawn() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 10 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(50));
            }
            result => return result,
        }
    }
}

/// What `out` says within `timeout`: up to its end, or up to and
/// including the first line that starts with `until`. A process that has
/// said what was asked but keeps its output open still counts, and the
/// reader stops there rather than blocking on it.
fn read_with_timeout(
    out: impl Read + Send + 'static,
    timeout: Duration,
    until: Option<&'static str>,
) -> Option<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        for line in BufReader::new(out).lines() {
            let Ok(line) = line else { break };
            let last = until.is_some_and(|mark| line.starts_with(mark));
            text.push_str(&line);
            text.push('\n');
            if last {
                break;
            }
        }
        let _ = tx.send(text);
    });
    rx.recv_timeout(timeout).ok()
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn symlink(_target: &Path, _link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "installing the CLI is not supported on this system",
    ))
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// What a new terminal sees: its PATH and what `pinrail` resolves to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShellView {
    pub path: Vec<PathBuf>,
    pub command: Option<PathBuf>,
}

#[cfg_attr(windows, allow(dead_code))]
const PATH_MARK: &str = "__pinrail_path__=";
#[cfg_attr(windows, allow(dead_code))]
const COMMAND_MARK: &str = "__pinrail_command__=";

/// Asks the user's login shell, the way a new terminal starts it. The app
/// inherits a much shorter PATH from the system than a terminal has, so its
/// own environment would say nothing useful. Marked lines, since shell
/// start-up files may print their own. None when the shell does not answer
/// in time.
pub fn ask_login_shell() -> Option<ShellView> {
    #[cfg(windows)]
    return windows_view();
    #[cfg(not(windows))]
    ask_unix_login_shell()
}

#[cfg(not(windows))]
fn ask_unix_login_shell() -> Option<ShellView> {
    #[cfg(unix)]
    let uid = rustix::process::getuid().as_raw();
    #[cfg(not(unix))]
    let uid = 0;
    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    ask_shell(&login_shell(std::env::var("SHELL").ok(), &passwd, uid))
}

/// The shell a new terminal starts: SHELL, which an app started from a
/// Linux desktop may not have, else the account's shell in `passwd`, else
/// `/bin/sh`, which every Unix has.
#[cfg_attr(windows, allow(dead_code))]
fn login_shell(variable: Option<String>, passwd: &str, uid: u32) -> String {
    if let Some(shell) = variable.filter(|s| !s.is_empty()) {
        return shell;
    }
    passwd
        .lines()
        .map(|line| line.split(':').collect::<Vec<_>>())
        .find(|fields| fields.len() >= 7 && fields[2].parse() == Ok(uid))
        .map(|fields| fields[6].to_string())
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| "/bin/sh".to_string())
}

#[cfg_attr(windows, allow(dead_code))]
fn ask_shell(shell: &str) -> Option<ShellView> {
    let script = format!(
        r#"printf '\n{PATH_MARK}%s\n' "$PATH"; printf '{COMMAND_MARK}%s\n' "$(command -v {COMMAND} 2>/dev/null)""#
    );
    let mut child = Command::new(shell)
        .args(["-ilc", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let text = read_with_timeout(stdout, SHELL_TIMEOUT, Some(COMMAND_MARK));
    // the answer is in, or the time is up: the shell has no more to say
    let _ = child.kill();
    let _ = child.wait();
    let text = text?;
    parse_shell(&text)
}

/// What a new terminal sees on Windows: the PATH the registry holds, the
/// machine's folders before the user's, and the first program called
/// `pinrail` on it, tried with each extension of PATHEXT in turn. None when
/// the registry cannot be read.
#[cfg(windows)]
fn windows_view() -> Option<ShellView> {
    let (machine, user) = registry_paths()?;
    let path = [machine, user]
        .into_iter()
        .flatten()
        .flat_map(|raw| split_registry_path(&raw))
        .collect::<Vec<_>>();
    let extensions = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    let command = path.iter().find_map(|dir| {
        extensions
            .split(';')
            .filter(|e| !e.is_empty())
            .map(|e| dir.join(format!("{PROGRAM}{e}")))
            .find(|p| p.is_file())
    });
    Some(ShellView { path, command })
}

/// The raw machine and user `Path` values, with their `%VARIABLES%`
/// unexpanded. The registry returns UTF-16, so the console's code page does
/// not matter. None when neither can be read.
#[cfg(windows)]
fn registry_paths() -> Option<(Option<String>, Option<String>)> {
    use windows_registry::{CURRENT_USER, LOCAL_MACHINE};
    let read =
        |key: &windows_registry::Key, path: &str| key.open(path).ok()?.get_string("Path").ok();
    let machine = read(
        LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
    );
    let user = read(CURRENT_USER, "Environment");
    (machine.is_some() || user.is_some()).then_some((machine, user))
}

/// The folders of a registry PATH value, `%VARIABLES%` expanded from the
/// environment; one that is not set stays as written.
#[cfg_attr(not(windows), allow(dead_code))]
fn split_registry_path(raw: &str) -> Vec<PathBuf> {
    raw.split(';')
        .filter(|s| !s.trim().is_empty())
        .map(|s| PathBuf::from(expand_variables(s.trim())))
        .collect()
}

#[cfg_attr(not(windows), allow(dead_code))]
fn expand_variables(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                match std::env::var(&after[..end]) {
                    Ok(value) => out.push_str(&value),
                    Err(_) => out.push_str(&rest[start..start + end + 2]),
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Puts `dir` at the end of the user's PATH, if it is not there, keeping the
/// `%VARIABLES%` the value already has, then tells running programs, such as
/// Explorer, that the environment changed, so that a terminal opened next
/// sees it. True when the PATH changed. A PATH value that is not a string is
/// left alone, since writing it back as one would change its meaning.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn add_to_user_path(dir: &Path) -> Result<bool, String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutA, WM_SETTINGCHANGE,
    };
    let fail = |e: String| format!("cannot add {} to the PATH: {e}", dir.display());
    let key = windows_registry::CURRENT_USER
        .create("Environment")
        .map_err(|e| fail(e.to_string()))?;
    // the value as written, with its %VARIABLES% unexpanded
    let raw = match key.get_value("Path") {
        Ok(value) => match value.ty() {
            windows_registry::Type::String | windows_registry::Type::ExpandString => {
                String::try_from(value).map_err(|e| fail(e.to_string()))?
            }
            _ => {
                return Err(format!(
                    "cannot add {} to the PATH: the user PATH is not a string value",
                    dir.display()
                ));
            }
        },
        // no PATH of the user's yet: ERROR_FILE_NOT_FOUND
        Err(e) if e.code().0 == 0x8007_0002_u32 as i32 => String::new(),
        Err(e) => return Err(fail(e.to_string())),
    };
    let wanted = dir.to_string_lossy();
    let wanted = wanted.trim_end_matches('\\');
    let same = |entry: &str| {
        expand_variables(entry.trim())
            .trim_end_matches('\\')
            .eq_ignore_ascii_case(wanted)
    };
    if raw.split(';').any(same) {
        return Ok(false);
    }
    let new = if raw.is_empty() {
        wanted.to_string()
    } else {
        format!("{};{wanted}", raw.trim_end_matches(';'))
    };
    key.set_expand_string("Path", &new)
        .map_err(|e| fail(e.to_string()))?;
    // tell Explorer and the rest that the environment changed, so that a
    // terminal opened next sees it
    // SAFETY: the message and the NUL-terminated string outlive the call.
    unsafe {
        SendMessageTimeoutA(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            c"Environment".as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            std::ptr::null_mut(),
        );
    }
    Ok(true)
}

#[cfg_attr(windows, allow(dead_code))]
fn parse_shell(text: &str) -> Option<ShellView> {
    let mut view = ShellView::default();
    let mut saw_path = false;
    for line in BufReader::new(text.as_bytes())
        .lines()
        .map_while(Result::ok)
    {
        if let Some(path) = line.strip_prefix(PATH_MARK) {
            view.path = std::env::split_paths(path).collect();
            saw_path = true;
        } else if let Some(command) = line.strip_prefix(COMMAND_MARK) {
            let command = command.trim();
            view.command = (!command.is_empty()).then(|| PathBuf::from(command));
        }
    }
    saw_path.then_some(view)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_registry_path_expands_the_variables_it_can() {
        let path = std::env::var("PATH").unwrap_or_default();
        assert_eq!(
            split_registry_path(r"%PATH%\bin;;%PINRAIL_UNSET_VAR%\x; C:\tools "),
            [
                PathBuf::from(format!(r"{path}\bin")),
                PathBuf::from(r"%PINRAIL_UNSET_VAR%\x"),
                PathBuf::from(r"C:\tools"),
            ]
        );
    }

    #[test]
    fn the_login_shell_is_the_variable_then_the_account_then_sh() {
        let passwd = "root:x:0:0:root:/root:/bin/bash\n\
                      pinrail:x:1000:1000::/home/pinrail:/usr/bin/fish\n";
        assert_eq!(
            login_shell(Some("/bin/zsh".into()), passwd, 1000),
            "/bin/zsh"
        );
        // an app started from a Linux desktop may have no SHELL
        assert_eq!(login_shell(None, passwd, 1000), "/usr/bin/fish");
        assert_eq!(
            login_shell(Some(String::new()), passwd, 1000),
            "/usr/bin/fish"
        );
        assert_eq!(login_shell(None, passwd, 1001), "/bin/sh");
    }

    #[cfg(unix)]
    #[test]
    fn a_shell_that_answers_then_keeps_running_still_counts() {
        // a start-up file that leaves something running holds the shell's
        // output open: the answer is in by the command line, not at its end
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let shell = dir.path().join("slow-shell");
        std::fs::write(
            &shell,
            format!("#!/bin/sh\nprintf '\\n{PATH_MARK}/usr/bin\\n{COMMAND_MARK}/usr/local/bin/pinrail\\n'\nexec sleep 10\n"),
        )
        .unwrap();
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();

        let started = std::time::Instant::now();
        let view = ask_shell(shell.to_str().unwrap()).expect("the shell answered");
        assert_eq!(view.command, Some(PathBuf::from("/usr/local/bin/pinrail")));
        assert!(
            started.elapsed() < SHELL_TIMEOUT,
            "it waited for the shell to exit"
        );
    }

    fn bundle(dir: &Path) -> PathBuf {
        let cli = dir.join("Pinrail.app/Contents/MacOS").join(SIDECAR);
        std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
        std::fs::write(&cli, "#!/bin/sh\n").unwrap();
        cli
    }

    #[test]
    #[cfg(unix)]
    fn install_links_the_bundled_cli_and_replaces_an_old_link() {
        let tmp = tempfile::tempdir().unwrap();
        let cli = bundle(tmp.path());
        let link = link_path(&tmp.path().join("home"));

        assert!(!status(Mode::Link, Some(&cli), &link, None).installed);
        install(Mode::Link, &cli, &link).unwrap();
        let now = status(Mode::Link, Some(&cli), &link, None);
        assert!(now.installed);
        assert_eq!(now.occupied_by, None);

        // a link left by an older copy of the app is replaced
        let old = tmp.path().join("old-pinrail");
        std::fs::write(&old, "").unwrap();
        std::fs::remove_file(&link).unwrap();
        symlink(&old, &link).unwrap();
        let before = status(Mode::Link, Some(&cli), &link, None);
        assert!(!before.installed);
        assert!(before.occupied_by.unwrap().starts_with("a link to "));
        install(Mode::Link, &cli, &link).unwrap();
        assert!(status(Mode::Link, Some(&cli), &link, None).installed);
    }

    #[test]
    fn a_file_in_the_links_place_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let cli = bundle(tmp.path());
        let link = link_path(&tmp.path().join("home"));
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::fs::write(&link, "someone else's pinrail").unwrap();

        let err = install(Mode::Link, &cli, &link).unwrap_err();
        assert!(err.contains("is a file, not a link"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&link).unwrap(),
            "someone else's pinrail"
        );
        assert_eq!(
            status(Mode::Link, Some(&cli), &link, None)
                .occupied_by
                .as_deref(),
            Some("a file")
        );
    }

    #[test]
    #[cfg(unix)]
    fn the_login_shells_answer_is_read_from_its_marked_lines() {
        let text = format!(
            "Welcome back!\n\n{PATH_MARK}/usr/bin:/home/me/.local/bin\n{COMMAND_MARK}/home/me/.local/bin/pinrail\n"
        );
        let view = parse_shell(&text).unwrap();
        assert_eq!(
            view.path,
            vec![
                PathBuf::from("/usr/bin"),
                PathBuf::from("/home/me/.local/bin")
            ]
        );
        assert_eq!(
            view.command,
            Some(PathBuf::from("/home/me/.local/bin/pinrail"))
        );

        let none = parse_shell(&format!("{PATH_MARK}/usr/bin\n{COMMAND_MARK}\n")).unwrap();
        assert_eq!(none.command, None);
        assert_eq!(parse_shell("no marks at all"), None);
    }

    #[test]
    #[cfg(unix)]
    fn status_says_whether_the_links_folder_is_on_the_path() {
        let tmp = tempfile::tempdir().unwrap();
        let cli = bundle(tmp.path());
        let link = link_path(&tmp.path().join("home"));
        install(Mode::Link, &cli, &link).unwrap();

        let on = ShellView {
            path: vec![link.parent().unwrap().to_path_buf()],
            command: Some(link.clone()),
        };
        let s = status(Mode::Link, Some(&cli), &link, Some(&on));
        assert_eq!(s.dir_on_path, Some(true));
        assert_eq!(s.runs, Some(link.clone()));

        let off = ShellView {
            path: vec![PathBuf::from("/usr/bin")],
            command: None,
        };
        assert_eq!(
            status(Mode::Link, Some(&cli), &link, Some(&off)).dir_on_path,
            Some(false)
        );
        assert_eq!(
            status(Mode::Link, Some(&cli), &link, None).dir_on_path,
            None
        );
    }

    /// A stand-in CLI: a script that answers `--version` the way `pinrail` does.
    #[cfg(unix)]
    fn script(path: &Path, version: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("#!/bin/sh\necho 'pinrail {version}'\n")).unwrap();
        executable(path).unwrap();
    }

    /// A copy of the CLI that is still open for writing when it is asked its
    /// version, as when another thread starts a process just after the
    /// copy was written, is still recognised once the file is closed.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_cli_that_is_briefly_busy_is_still_recognised() {
        let dir = tempfile::tempdir().unwrap();
        let cli = dir.path().join(COMMAND);
        script(&cli, "0.1.0");
        let open = std::fs::OpenOptions::new().append(true).open(&cli).unwrap();
        let closing = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            drop(open);
        });
        assert!(is_pinrail_cli(&cli));
        closing.join().unwrap();
    }

    /// After an update, the AppImage's copy of the CLI is brought up to the
    /// bundled one; a copy nobody installed is not made, and a file that is
    /// not the CLI is left alone.
    #[cfg(unix)]
    #[test]
    fn an_appimage_update_refreshes_the_copy_it_installed() {
        let dir = tempfile::tempdir().unwrap();
        let bundled = dir.path().join("app").join(COMMAND);
        script(&bundled, "0.2.0");
        let link = link_path(&dir.path().join("home"));

        assert_eq!(
            refresh(&bundled, &link),
            Ok(false),
            "nothing installed, nothing made"
        );
        assert!(!link.exists());

        script(&link, "0.1.0");
        assert_eq!(refresh(&bundled, &link), Ok(true));
        assert!(same_contents(&bundled, &link));
        assert_eq!(
            refresh(&bundled, &link),
            Ok(false),
            "already the bundled one"
        );

        std::fs::write(&link, "#!/bin/sh\necho 'something else'\n").unwrap();
        assert_eq!(refresh(&bundled, &link), Ok(false));
        assert!(
            std::fs::read_to_string(&link)
                .unwrap()
                .contains("something else")
        );
    }

    #[test]
    #[cfg(windows)]
    fn windows_always_copies_the_cli() {
        let sidecar = Path::new(r"C:\Program Files\Pinrail\pinrail-cli.exe");
        assert_eq!(mode(sidecar, false), Mode::Copy);
    }

    #[test]
    #[cfg(unix)]
    fn the_mode_follows_the_clis_name_and_the_appimage() {
        assert_eq!(
            mode(
                Path::new("/Applications/Pinrail.app/Contents/MacOS/pinrail-cli"),
                false
            ),
            Mode::Link
        );
        assert_eq!(mode(Path::new("/usr/bin/pinrail"), false), Mode::Package);
        assert_eq!(
            mode(Path::new("/tmp/.mount_PinrailX/usr/bin/pinrail"), true),
            Mode::Copy
        );
    }

    #[test]
    fn a_package_has_nothing_to_install() {
        let tmp = tempfile::tempdir().unwrap();
        let cli = tmp.path().join("usr/bin/pinrail");
        std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
        std::fs::write(&cli, "").unwrap();
        let link = link_path(&tmp.path().join("home"));

        let s = status(Mode::Package, Some(&cli), &link, None);
        assert!(s.installed);
        assert_eq!(s.link, cli);
        install(Mode::Package, &cli, &link).unwrap();
        assert!(!link.exists());
    }

    #[cfg(unix)]
    #[test]
    fn an_appimage_copies_the_cli_and_replaces_an_older_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let cli = tmp.path().join("mount/usr/bin/pinrail");
        script(&cli, "0.2.0");
        let link = link_path(&tmp.path().join("home"));

        install(Mode::Copy, &cli, &link).unwrap();
        let now = status(Mode::Copy, Some(&cli), &link, None);
        assert!(now.installed && !now.outdated, "{now:?}");
        assert!(
            !std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );

        // a copy from an older AppImage
        script(&link, "0.1.0");
        let before = status(Mode::Copy, Some(&cli), &link, None);
        assert!(
            !before.installed && before.outdated && before.occupied_by.is_none(),
            "{before:?}"
        );
        install(Mode::Copy, &cli, &link).unwrap();
        assert!(status(Mode::Copy, Some(&cli), &link, None).installed);
        assert_eq!(std::fs::read(&link).unwrap(), std::fs::read(&cli).unwrap());

        // a link left by a link install is replaced, not written through
        let other = tmp.path().join("other");
        std::fs::write(&other, "keep").unwrap();
        std::fs::remove_file(&link).unwrap();
        symlink(&other, &link).unwrap();
        install(Mode::Copy, &cli, &link).unwrap();
        assert_eq!(std::fs::read_to_string(&other).unwrap(), "keep");
        assert!(status(Mode::Copy, Some(&cli), &link, None).installed);
    }

    #[cfg(unix)]
    #[test]
    fn an_appimage_leaves_a_file_that_is_not_the_cli_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let cli = tmp.path().join("mount/usr/bin/pinrail");
        script(&cli, "0.2.0");
        let link = link_path(&tmp.path().join("home"));
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::fs::write(&link, "someone else's pinrail").unwrap();

        let s = status(Mode::Copy, Some(&cli), &link, None);
        assert_eq!(s.occupied_by.as_deref(), Some("a file"));
        assert!(!s.outdated);
        let err = install(Mode::Copy, &cli, &link).unwrap_err();
        assert!(err.contains("not the pinrail CLI"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&link).unwrap(),
            "someone else's pinrail"
        );
    }
}
