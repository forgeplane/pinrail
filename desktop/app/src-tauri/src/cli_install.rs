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
    /// the file the person's shell reads at start, where Pinrail can add the
    /// line that puts the install's folder on the PATH; none for a shell it
    /// does not know, and on Windows
    pub profile: Option<PathBuf>,
    /// that line, in the shell's syntax
    pub profile_line: Option<String>,
    /// whether the PATH is the user's value in the registry, as on Windows,
    /// which Pinrail can add the folder to, rather than a shell's file
    pub user_path: bool,
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
        // the installed CLI by its own name, however the PATH spells it:
        // with other capitals on Windows, where PATHEXT gives `.EXE`, or
        // through `.`; the window compares the two as text
        runs: shell.and_then(|s| s.command.clone()).map(|runs| {
            if same_file(&runs, link) {
                link.to_path_buf()
            } else {
                runs
            }
        }),
        warning: None,
        profile: None,
        profile_line: None,
        user_path: cfg!(windows),
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
    let old = link.with_file_name(format!(".{PROGRAM}.old"));
    // a copy an earlier install moved aside, once nothing runs it
    let _ = std::fs::remove_file(&old);
    match std::fs::rename(partial, link) {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && link.is_file() => {
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

/// Starts `command` without a console window of its own on Windows, which
/// the app, having no console, would otherwise open for a console program.
pub(crate) fn without_window(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(
        command,
        windows_sys::Win32::System::Threading::CREATE_NO_WINDOW,
    );
    command
}

/// Whether `path` is some version of the pinrail CLI: it says `pinrail <version>`
/// when asked. Anything that does not answer in time is not.
fn is_pinrail_cli(path: &Path) -> bool {
    let Ok(mut child) = spawn(
        without_window(&mut Command::new(path))
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
    ask_shell(&this_login_shell())
}

/// The shell a new terminal starts for the person.
#[cfg(not(windows))]
pub fn this_login_shell() -> String {
    #[cfg(unix)]
    let uid = rustix::process::getuid().as_raw();
    #[cfg(not(unix))]
    let uid = 0;
    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    login_shell(std::env::var("SHELL").ok(), &passwd, uid)
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
    let command = find_command(&path, &extensions);
    Some(ShellView { path, command })
}

/// The first program called `pinrail` in the folders of `path`, tried with
/// each extension of `extensions`, a list such as PATHEXT holds.
#[cfg_attr(not(windows), allow(dead_code))]
fn find_command(path: &[PathBuf], extensions: &str) -> Option<PathBuf> {
    path.iter().find_map(|dir| {
        extensions
            .split(';')
            .filter(|e| !e.is_empty())
            .map(|e| dir.join(format!("{PROGRAM}{e}")))
            .find(|p| p.is_file())
    })
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

/// A PowerShell line that puts `dir` at the end of the user's PATH, for
/// someone to run when Pinrail could not: the value in the registry, which
/// a new terminal reads, not the running shell's. It reads and writes the
/// value as it is stored, so the `%VARIABLES%` in it stay variables, and
/// then sets and removes a variable through .NET, which tells running
/// programs, such as Explorer, that the environment changed.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn powershell_path_line(dir: &Path) -> String {
    path_line_for("Path", dir)
}

/// The line, for the user variable `name`.
#[cfg_attr(not(windows), allow(dead_code))]
fn path_line_for(name: &str, dir: &Path) -> String {
    let dir = dir.display().to_string().replace('\'', "''");
    format!(
        "$k = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment'); \
         $p = [string]$k.GetValue('{name}', '', 'DoNotExpandEnvironmentNames'); \
         $k.SetValue('{name}', (@($p.TrimEnd(';'), '{dir}') -ne '') -join ';', 'ExpandString'); \
         [Environment]::SetEnvironmentVariable('PinrailRefresh', '1', 'User'); \
         [Environment]::SetEnvironmentVariable('PinrailRefresh', $null, 'User')"
    )
}

/// Puts `dir` at the end of the user's PATH, if it is not there, keeping the
/// `%VARIABLES%` the value already has, then tells running programs, such as
/// Explorer, that the environment changed, so that a terminal opened next
/// sees it. True when the PATH changed. A PATH value that is not a string is
/// left alone, since writing it back as one would change its meaning.
#[cfg(windows)]
pub fn add_to_user_path(dir: &Path) -> Result<bool, String> {
    let key = windows_registry::CURRENT_USER
        .create("Environment")
        .map_err(|e| format!("cannot add {} to the PATH: {e}", dir.display()))?;
    if !add_to_path_in(&key, dir)? {
        return Ok(false);
    }
    environment_changed();
    Ok(true)
}

/// Takes `dir` out of the user's PATH, leaving every other entry as it is
/// written, then tells running programs that the environment changed. True
/// when the PATH changed.
#[cfg(windows)]
pub fn remove_from_user_path(dir: &Path) -> Result<bool, String> {
    let key = windows_registry::CURRENT_USER
        .create("Environment")
        .map_err(|e| format!("cannot take {} off the PATH: {e}", dir.display()))?;
    if !remove_from_path_in(&key, dir)? {
        return Ok(false);
    }
    environment_changed();
    Ok(true)
}

/// Tells Explorer and the rest that the environment changed, so that a
/// terminal opened next sees it.
#[cfg(windows)]
#[allow(unsafe_code)]
fn environment_changed() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutA, WM_SETTINGCHANGE,
    };
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
}

/// The `Path` value of `key` as written, with its `%VARIABLES%` unexpanded,
/// and whether it is stored as one that expands them; empty when there is
/// none. A value that is not a string is refused, since writing it back as
/// one would change its meaning: `doing` says what could not be done.
#[cfg(windows)]
fn raw_path_in(key: &windows_registry::Key, doing: &str) -> Result<(String, bool), String> {
    match key.get_value("Path") {
        Ok(value) => {
            let expands = match value.ty() {
                windows_registry::Type::String => false,
                windows_registry::Type::ExpandString => true,
                _ => return Err(format!("{doing}: the user PATH is not a string value")),
            };
            String::try_from(value)
                .map(|raw| (raw, expands))
                .map_err(|e| format!("{doing}: {e}"))
        }
        // no PATH of the user's yet: ERROR_FILE_NOT_FOUND
        Err(e) if e.code().0 == 0x8007_0002_u32 as i32 => Ok((String::new(), true)),
        Err(e) => Err(format!("{doing}: {e}")),
    }
}

/// Puts `dir` at the end of the `Path` value in `key`, as [`add_to_user_path`]
/// does in the user's environment. True when the value changed.
#[cfg(windows)]
fn add_to_path_in(key: &windows_registry::Key, dir: &Path) -> Result<bool, String> {
    let doing = format!("cannot add {} to the PATH", dir.display());
    let (raw, _) = raw_path_in(key, &doing)?;
    let Some(new) = with_path_entry(&raw, &dir.to_string_lossy()) else {
        return Ok(false);
    };
    key.set_expand_string("Path", &new)
        .map_err(|e| format!("{doing}: {e}"))?;
    Ok(true)
}

/// Takes `dir` out of the `Path` value in `key`, as
/// [`remove_from_user_path`] does in the user's environment: the value keeps
/// its type, and is removed when `dir` was all it held, as there was none
/// before Pinrail added to it. True when the value changed.
#[cfg(windows)]
fn remove_from_path_in(key: &windows_registry::Key, dir: &Path) -> Result<bool, String> {
    let doing = format!("cannot take {} off the PATH", dir.display());
    let (raw, expands) = raw_path_in(key, &doing)?;
    let Some(new) = without_path_entry(&raw, &dir.to_string_lossy()) else {
        return Ok(false);
    };
    let written = if new.is_empty() {
        key.remove_value("Path")
    } else if expands {
        key.set_expand_string("Path", &new)
    } else {
        key.set_string("Path", &new)
    };
    written.map_err(|e| format!("{doing}: {e}"))?;
    Ok(true)
}

/// Whether `entry`, one of a registry PATH's, names the folder `dir`: with
/// its `%VARIABLES%` expanded, whatever its capitals, with or without a
/// backslash at its end.
#[cfg_attr(not(windows), allow(dead_code))]
fn is_path_entry(entry: &str, dir: &str) -> bool {
    expand_variables(entry.trim())
        .trim_end_matches('\\')
        .eq_ignore_ascii_case(dir.trim_end_matches('\\'))
}

/// The registry PATH `raw` with `dir` at its end; none when `raw` has it
/// already.
#[cfg_attr(not(windows), allow(dead_code))]
fn with_path_entry(raw: &str, dir: &str) -> Option<String> {
    if raw.split(';').any(|entry| is_path_entry(entry, dir)) {
        return None;
    }
    let dir = dir.trim_end_matches('\\');
    Some(if raw.is_empty() {
        dir.to_string()
    } else {
        format!("{};{dir}", raw.trim_end_matches(';'))
    })
}

/// The registry PATH `raw` without its entries for `dir`, the others as
/// they are written and in their order; none when `raw` has no such entry.
#[cfg_attr(not(windows), allow(dead_code))]
fn without_path_entry(raw: &str, dir: &str) -> Option<String> {
    if !raw.split(';').any(|entry| is_path_entry(entry, dir)) {
        return None;
    }
    Some(
        raw.split(';')
            .filter(|entry| !is_path_entry(entry, dir))
            .collect::<Vec<_>>()
            .join(";"),
    )
}

/// The file in Pinrail's data folder that names the folder Pinrail put on
/// the user's PATH. The uninstaller takes off the PATH only an entry
/// recorded here: one that the person added, or another program that keeps
/// its commands in `~\.local\bin`, is theirs.
#[cfg_attr(not(windows), allow(dead_code))]
const PATH_RECORD: &str = "cli-path-entry";

/// Records in `data_dir` that Pinrail put `dir` on the user's PATH.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn record_path_entry(data_dir: &Path, dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(data_dir.join(PATH_RECORD), dir.to_string_lossy().as_bytes())
}

/// The folder Pinrail put on the user's PATH, as `data_dir` records it.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn recorded_path_entry(data_dir: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(data_dir.join(PATH_RECORD)).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| PathBuf::from(text))
}

/// Forgets the folder [`record_path_entry`] recorded.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn forget_path_entry(data_dir: &Path) {
    let _ = std::fs::remove_file(data_dir.join(PATH_RECORD));
}

/// Removes the copy of the CLI at `link` that [`Mode::Copy`] installed, and
/// what installing leaves beside it. The copy is told as installing tells
/// one it may replace: it matches the bundled CLI, or it is some version of
/// the pinrail CLI. A link, a folder and any other file stay. True when a
/// copy was removed.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn remove_copy(bundled: Option<&Path>, link: &Path) -> Result<bool, String> {
    for beside in ["partial", "old"] {
        let _ = std::fs::remove_file(link.with_file_name(format!(".{PROGRAM}.{beside}")));
    }
    let ours = match std::fs::symlink_metadata(link) {
        Ok(meta) if meta.is_file() => {
            bundled.is_some_and(|b| same_contents(b, link)) || is_pinrail_cli(link)
        }
        _ => false,
    };
    if !ours {
        return Ok(false);
    }
    std::fs::remove_file(link)
        .map(|()| true)
        .map_err(|e| format!("cannot remove {}: {e}", link.display()))
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
    #[test]
    fn the_powershell_line_sets_the_users_path_with_the_folder_quoted() {
        let line = super::powershell_path_line(Path::new(r"C:\Users\o'neil\.local\bin"));
        assert!(line.contains(r"'C:\Users\o''neil\.local\bin'"), "{line}");
        assert!(line.contains("GetValue('Path', ''"), "{line}");
        assert!(line.contains("SetValue('Path', "), "{line}");
    }

    use super::*;

    /// Runs the PowerShell line for the user variable `name`, set to
    /// `value` first, or unset, and gives what the registry then holds:
    /// its type and its text, as written.
    #[cfg(windows)]
    fn after_the_line(
        name: &str,
        value: Option<&str>,
        dir: &str,
    ) -> (windows_registry::Type, String) {
        let env = windows_registry::CURRENT_USER
            .create("Environment")
            .unwrap();
        match value {
            Some(value) => env.set_expand_string(name, value).unwrap(),
            None => {
                let _ = env.remove_value(name);
            }
        }
        let line = path_line_for(name, Path::new(dir));
        let status = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &line])
            .status()
            .unwrap();
        assert!(status.success(), "{line}");
        let found = (env.get_type(name).unwrap(), env.get_string(name).unwrap());
        env.remove_value(name).unwrap();
        found
    }

    #[test]
    #[cfg(windows)]
    fn the_powershell_line_keeps_the_variables_the_path_names() {
        let (kind, value) =
            after_the_line("PinrailTestPath", Some(r"%USERPROFILE%\tools"), r"C:\x");
        assert_eq!(value, r"%USERPROFILE%\tools;C:\x");
        assert_eq!(kind, windows_registry::Type::ExpandString);
    }

    #[test]
    #[cfg(windows)]
    fn the_powershell_line_starts_a_path_that_was_not_there() {
        assert_eq!(after_the_line("PinrailTestPath2", None, r"C:\x").1, r"C:\x");
    }

    #[test]
    fn a_folder_is_added_to_a_registry_path_once_and_taken_out_again() {
        let dir = r"C:\Users\p\.local\bin";
        let before = r"%USERPROFILE%\tools;C:\x";
        let after = with_path_entry(before, dir).unwrap();
        assert_eq!(after, r"%USERPROFILE%\tools;C:\x;C:\Users\p\.local\bin");
        assert_eq!(with_path_entry(&after, dir), None, "already there");
        assert_eq!(without_path_entry(&after, dir).as_deref(), Some(before));
        assert_eq!(without_path_entry(before, dir), None, "nothing to take");

        // a PATH that Pinrail started holds nothing once the folder is out
        let alone = with_path_entry("", dir).unwrap();
        assert_eq!(alone, dir);
        assert_eq!(without_path_entry(&alone, dir).as_deref(), Some(""));
    }

    #[test]
    fn taking_a_folder_out_of_a_registry_path_leaves_the_rest_as_written() {
        let dir = r"C:\Users\p\.local\bin";
        // other capitals, a backslash at the end, spaces, and twice
        let raw = r"%PINRAIL_UNSET_VAR%\x; C:\Tools ;c:\users\P\.LOCAL\bin\;;C:\Users\p\.local\bin;C:\Users\p\.local\bin2";
        assert_eq!(
            without_path_entry(raw, dir).as_deref(),
            Some(r"%PINRAIL_UNSET_VAR%\x; C:\Tools ;;C:\Users\p\.local\bin2")
        );
    }

    #[test]
    #[cfg(windows)]
    fn the_registry_path_gets_a_folder_and_loses_only_that_folder() {
        let key = windows_registry::CURRENT_USER
            .create(r"Software\PinrailTest\PathEntry")
            .unwrap();
        let dir = Path::new(r"C:\Users\p\.local\bin");

        // none before: one is made, and removed with its only folder
        let _ = key.remove_value("Path");
        assert_eq!(add_to_path_in(&key, dir), Ok(true));
        assert_eq!(key.get_string("Path").unwrap(), r"C:\Users\p\.local\bin");
        assert_eq!(remove_from_path_in(&key, dir), Ok(true));
        assert!(key.get_value("Path").is_err());
        assert_eq!(remove_from_path_in(&key, dir), Ok(false));

        // the others stay as written, and the value keeps its type
        key.set_expand_string("Path", r"%USERPROFILE%\tools;C:\x")
            .unwrap();
        assert_eq!(add_to_path_in(&key, dir), Ok(true));
        assert_eq!(add_to_path_in(&key, dir), Ok(false));
        assert_eq!(remove_from_path_in(&key, dir), Ok(true));
        assert_eq!(key.get_string("Path").unwrap(), r"%USERPROFILE%\tools;C:\x");
        assert_eq!(
            key.get_type("Path").unwrap(),
            windows_registry::Type::ExpandString
        );
        key.set_string("Path", r"C:\x;C:\Users\p\.local\bin\")
            .unwrap();
        assert_eq!(remove_from_path_in(&key, dir), Ok(true));
        assert_eq!(key.get_string("Path").unwrap(), r"C:\x");
        assert_eq!(
            key.get_type("Path").unwrap(),
            windows_registry::Type::String
        );

        windows_registry::CURRENT_USER
            .remove_tree(r"Software\PinrailTest")
            .unwrap();
    }

    #[test]
    fn the_folder_put_on_the_path_is_recorded_and_forgotten() {
        let data = tempfile::tempdir().unwrap();
        let data_dir = data.path().join("data");
        assert_eq!(recorded_path_entry(&data_dir), None);
        record_path_entry(&data_dir, Path::new(r"C:\Users\p\.local\bin")).unwrap();
        assert_eq!(
            recorded_path_entry(&data_dir),
            Some(PathBuf::from(r"C:\Users\p\.local\bin"))
        );
        forget_path_entry(&data_dir);
        assert_eq!(recorded_path_entry(&data_dir), None);
    }

    /// Only the pinrail CLI is removed from the install's place: the copy
    /// of the bundled one or an older one, with what an install left beside
    /// it; not a file that is something else, a link or a folder.
    #[cfg(unix)]
    #[test]
    fn only_a_copy_of_the_cli_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let bundled = dir.path().join("app").join(COMMAND);
        script(&bundled, "0.2.0");
        let link = link_path(&dir.path().join("home"));
        let bin = link.parent().unwrap();

        assert_eq!(remove_copy(Some(&bundled), &link), Ok(false), "none there");

        install(Mode::Copy, &bundled, &link).unwrap();
        let old = bin.join(".pinrail.old");
        std::fs::write(&old, "").unwrap();
        assert_eq!(remove_copy(Some(&bundled), &link), Ok(true));
        assert!(!link.exists() && !old.exists());

        script(&link, "0.1.0");
        assert_eq!(remove_copy(None, &link), Ok(true), "an older copy");
        assert!(!link.exists());

        std::fs::write(&link, "#!/bin/sh\necho 'something else'\n").unwrap();
        executable(&link).unwrap();
        assert_eq!(remove_copy(Some(&bundled), &link), Ok(false));
        assert!(link.is_file());

        std::fs::remove_file(&link).unwrap();
        symlink(&bundled, &link).unwrap();
        assert_eq!(remove_copy(Some(&bundled), &link), Ok(false));
        assert!(link.is_symlink());

        std::fs::remove_file(&link).unwrap();
        std::fs::create_dir(&link).unwrap();
        assert_eq!(remove_copy(Some(&bundled), &link), Ok(false));
        assert!(link.is_dir());
    }

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
        // written once, in a folder that outlives the test: macOS scans a
        // new executable the first time it runs, longer than a shell has
        let content = format!(
            "#!/bin/sh\nprintf '\\n{PATH_MARK}/usr/bin\\n{COMMAND_MARK}/usr/local/bin/pinrail\\n'\nexec sleep 10\n"
        );
        let shell = std::env::temp_dir().join("pinrail-test-slow-shell");
        if std::fs::read_to_string(&shell).ok().as_deref() != Some(content.as_str()) {
            // written beside it and renamed, as another run may run it meanwhile
            let fresh = shell.with_extension(std::process::id().to_string());
            std::fs::write(&fresh, &content).unwrap();
            std::fs::set_permissions(&fresh, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::rename(&fresh, &shell).unwrap();
        }

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

    /// The installed CLI in a folder of its own, and that folder.
    fn installed_file(root: &Path) -> (PathBuf, PathBuf) {
        let link = link_path(&root.join("home"));
        let dir = link.parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&link, b"cli").unwrap();
        (link, dir)
    }

    #[test]
    fn the_installed_cli_by_another_spelling_is_the_one_a_terminal_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let (link, dir) = installed_file(tmp.path());
        // the same file, as a PATH entry can spell its folder
        let view = ShellView {
            path: vec![dir.join(".")],
            command: Some(dir.join(".").join(link.file_name().unwrap())),
        };
        let s = status(Mode::Copy, None, &link, Some(&view));
        // as the window compares them: as text
        assert_eq!(text(s.runs.as_deref()), text(Some(&link)));
        assert_eq!(s.dir_on_path, Some(true));
    }

    #[test]
    #[cfg(windows)]
    fn the_cli_found_through_pathext_is_the_installed_one() {
        let tmp = tempfile::tempdir().unwrap();
        let (link, dir) = installed_file(tmp.path());
        // Windows' PATHEXT has its extensions in capitals
        let found = find_command(std::slice::from_ref(&dir), ".COM;.EXE;.BAT;.CMD").unwrap();
        let view = ShellView {
            path: vec![dir],
            command: Some(found),
        };
        let runs = status(Mode::Copy, None, &link, Some(&view)).runs;
        assert_eq!(text(runs.as_deref()), text(Some(&link)));
    }

    /// A path as the window gets it, in JSON.
    fn text(path: Option<&Path>) -> Option<String> {
        path.map(|p| p.as_os_str().to_string_lossy().into_owned())
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
    fn the_command_is_found_with_the_first_extension_that_names_a_file() {
        let tmp = tempfile::tempdir().unwrap();
        let (empty, tools) = (tmp.path().join("empty"), tmp.path().join("tools"));
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::write(tools.join(format!("{PROGRAM}.cmd")), "").unwrap();
        let path = [empty, tools.clone()];
        assert_eq!(
            find_command(&path, ".COM;.EXE;.BAT;.cmd;"),
            Some(tools.join(format!("{PROGRAM}.cmd")))
        );
        assert_eq!(find_command(&path, ".COM;.EXE"), None);
    }

    /// A copy of one of Windows' own programs at `to`.
    #[cfg(windows)]
    fn system_program(name: &str, to: &Path) {
        let root = std::env::var_os("SystemRoot").unwrap();
        std::fs::copy(PathBuf::from(root).join("System32").join(name), to).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn a_copy_replaces_a_running_cli_and_the_next_install_removes_the_old_one() {
        let tmp = tempfile::tempdir().unwrap();
        let link = tmp.path().join(COMMAND);
        system_program("PING.EXE", &link);
        // the installed copy runs, as a `pinrail wait` would
        let mut running = Command::new(&link)
            .args(["-n", "30", "127.0.0.1"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let bundled = tmp.path().join("bundled.exe");
        system_program("WHOAMI.EXE", &bundled);

        copy(&bundled, &link).unwrap();
        assert!(same_contents(&bundled, &link));
        assert!(
            running.try_wait().unwrap().is_none(),
            "the old copy still runs"
        );
        let old = link.with_file_name(format!(".{PROGRAM}.old"));
        assert!(old.is_file(), "the running copy was moved aside");

        running.kill().unwrap();
        running.wait().unwrap();
        copy(&bundled, &link).unwrap();
        assert!(!old.exists(), "the next install removes the old copy");
    }

    #[test]
    #[cfg(windows)]
    fn on_windows_install_copies_the_cli_and_leaves_another_file_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let cli = bundle(tmp.path());
        let link = link_path(&tmp.path().join("home"));
        assert!(link.ends_with(r".local\bin\pinrail.exe"));
        install(Mode::Copy, &cli, &link).unwrap();
        assert!(status(Mode::Copy, Some(&cli), &link, None).installed);

        let other = link_path(&tmp.path().join("other"));
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        std::fs::write(&other, "not pinrail").unwrap();
        let error = install(Mode::Copy, &cli, &other).unwrap_err();
        assert!(error.contains("not the pinrail CLI"), "{error}");
        assert_eq!(std::fs::read(&other).unwrap(), b"not pinrail");
    }

    /// A registry key of the test's own under the user's, removed afterwards.
    #[cfg(windows)]
    struct ScratchKey {
        path: String,
        key: windows_registry::Key,
    }

    #[cfg(windows)]
    impl ScratchKey {
        fn new(test: &str) -> Self {
            let path = format!(r"Software\Pinrail\tests\{test}-{}", std::process::id());
            let key = windows_registry::CURRENT_USER.create(&path).unwrap();
            Self { path, key }
        }
    }

    #[cfg(windows)]
    impl Drop for ScratchKey {
        fn drop(&mut self) {
            let _ = windows_registry::CURRENT_USER.remove_tree(&self.path);
        }
    }

    /// The folder the install puts on the PATH.
    #[cfg(windows)]
    fn local_bin() -> PathBuf {
        PathBuf::from(std::env::var_os("USERPROFILE").unwrap()).join(r".local\bin")
    }

    #[test]
    #[cfg(windows)]
    fn the_folder_is_added_at_the_end_of_the_path_with_its_variables_kept() {
        let scratch = ScratchKey::new("append");
        let dir = local_bin();
        // no value yet
        assert_eq!(add_to_path_in(&scratch.key, &dir), Ok(true));
        assert_eq!(
            scratch.key.get_string("Path").unwrap(),
            dir.to_string_lossy()
        );

        scratch
            .key
            .set_expand_string("Path", r"%SystemRoot%\tools;C:\bin;")
            .unwrap();
        assert_eq!(add_to_path_in(&scratch.key, &dir), Ok(true));
        let value = scratch.key.get_value("Path").unwrap();
        assert_eq!(value.ty(), windows_registry::Type::ExpandString);
        assert_eq!(
            String::try_from(value).unwrap(),
            format!(r"%SystemRoot%\tools;C:\bin;{}", dir.display())
        );
    }

    #[test]
    #[cfg(windows)]
    fn a_folder_already_on_the_path_is_not_added_again() {
        let scratch = ScratchKey::new("present");
        let dir = local_bin();
        let written = [
            dir.to_string_lossy().to_uppercase(),
            format!(r"{}\", dir.display()),
            r"%USERPROFILE%\.local\bin".to_string(),
        ];
        for entry in written {
            let value = format!(r"C:\bin;{entry}");
            scratch.key.set_expand_string("Path", &value).unwrap();
            assert_eq!(add_to_path_in(&scratch.key, &dir), Ok(false), "{entry}");
            assert_eq!(scratch.key.get_string("Path").unwrap(), value);
        }
    }

    #[test]
    #[cfg(windows)]
    fn a_path_that_is_not_a_string_is_left_alone() {
        let scratch = ScratchKey::new("number");
        scratch.key.set_u32("Path", 7).unwrap();
        let error = add_to_path_in(&scratch.key, &local_bin()).unwrap_err();
        assert!(error.contains("not a string"), "{error}");
        assert_eq!(scratch.key.get_u32("Path").unwrap(), 7);
    }

    #[test]
    #[cfg(windows)]
    fn the_path_a_new_terminal_sees_is_read_from_the_registry() {
        let view = windows_view().expect("the registry can be read");
        assert!(
            view.path
                .iter()
                .any(|p| p.to_string_lossy().to_lowercase().ends_with(r"\system32")),
            "{:?}",
            view.path
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
