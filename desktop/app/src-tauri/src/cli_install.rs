//! Putting the bundled CLI on the PATH, which depends on how the app came.
//!
//! - macOS: the bundle carries the CLI as `wicket-cli` beside `Wicket`.
//!   Installing links `~/.local/bin/wicket` to it, so updating the app
//!   updates the CLI and nothing is copied.
//! - A Linux package (.deb, .rpm) installs the CLI as `/usr/bin/wicket` beside
//!   `wicket-desktop`; it is on the PATH already and there is nothing to do.
//! - An AppImage carries the same `wicket`, but mounts itself at a new
//!   temporary path each run, so a link would break: installing copies it to
//!   `~/.local/bin/wicket` instead, and a copy from an older AppImage can be
//!   replaced by installing again.
//!
//! A development build has no bundled CLI.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde::Serialize;

/// The CLI's name in a macOS bundle, beside `Wicket`.
const SIDECAR: &str = "wicket-cli";
/// The name a shell runs it by, and its name in the Linux builds.
const COMMAND: &str = "wicket";
/// How long a login shell may take to say what its PATH is.
const SHELL_TIMEOUT: Duration = Duration::from_secs(4);
/// How long `wicket --version` may take when telling an old copy apart.
const VERSION_TIMEOUT: Duration = Duration::from_secs(2);

/// How the bundled CLI reaches the PATH.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// `~/.local/bin/wicket` links to it
    Link,
    /// a Linux package put it on the PATH; nothing to install
    Package,
    /// `~/.local/bin/wicket` is a copy of it, since its own path is temporary
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
    /// what `wicket` runs in a new terminal, when the login shell said
    pub runs: Option<PathBuf>,
    /// whether the install's folder is on a new terminal's PATH; none when
    /// the login shell could not be asked
    pub dir_on_path: Option<bool>,
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
/// package's own file for a CLI named `wicket`, and a link otherwise.
pub fn mode(bundled: &Path, appimage: bool) -> Mode {
    if appimage {
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

/// `~/.local/bin/wicket`.
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
            } else if is_wicket_cli(link) {
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
        Ok(_) if mode == Mode::Copy && is_wicket_cli(link) => true,
        Ok(_) if mode == Mode::Copy => {
            return Err(format!(
                "{} is a file that is not the wicket CLI; move it aside and install again",
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

/// Copies beside `link` and renames over it, so a terminal never runs half a
/// file, and a link in its place is replaced rather than written through.
fn copy(bundled: &Path, link: &Path) -> Result<(), String> {
    let partial = link.with_file_name(format!(".{COMMAND}.partial"));
    let result = std::fs::copy(bundled, &partial)
        .and_then(|_| executable(&partial))
        .and_then(|_| std::fs::rename(&partial, link));
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result.map_err(|e| format!("cannot copy the CLI to {}: {e}", link.display()))
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

/// Whether `path` is some version of the wicket CLI: it says `wicket <version>`
/// when asked. Anything that does not answer in time is not.
fn is_wicket_cli(path: &Path) -> bool {
    let Ok(mut child) = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let Some(stdout) = child.stdout.take() else {
        return false;
    };
    let text = read_with_timeout(stdout, VERSION_TIMEOUT);
    if text.is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
    text.is_some_and(|t| t.starts_with(&format!("{COMMAND} ")))
}

fn read_with_timeout(mut out: impl Read + Send + 'static, timeout: Duration) -> Option<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = out.read_to_string(&mut text);
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

/// What a new terminal sees: its PATH and what `wicket` resolves to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShellView {
    pub path: Vec<PathBuf>,
    pub command: Option<PathBuf>,
}

const PATH_MARK: &str = "__wicket_path__=";
const COMMAND_MARK: &str = "__wicket_command__=";

/// Asks the user's login shell, the way a new terminal starts it. The app
/// inherits a much shorter PATH from the system than a terminal has, so its
/// own environment would say nothing useful. Marked lines, since shell
/// start-up files may print their own. None when the shell does not answer
/// in time.
pub fn ask_login_shell() -> Option<ShellView> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let script = format!(
        r#"printf '\n{PATH_MARK}%s\n' "$PATH"; printf '{COMMAND_MARK}%s\n' "$(command -v {COMMAND} 2>/dev/null)""#
    );
    let mut child = Command::new(&shell)
        .args(["-ilc", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let Some(text) = read_with_timeout(stdout, SHELL_TIMEOUT) else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    let _ = child.wait();
    parse_shell(&text)
}

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

    fn bundle(dir: &Path) -> PathBuf {
        let cli = dir.join("Wicket.app/Contents/MacOS").join(SIDECAR);
        std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
        std::fs::write(&cli, "#!/bin/sh\n").unwrap();
        cli
    }

    #[test]
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
        let old = tmp.path().join("old-wicket");
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
        std::fs::write(&link, "someone else's wicket").unwrap();

        let err = install(Mode::Link, &cli, &link).unwrap_err();
        assert!(err.contains("is a file, not a link"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&link).unwrap(),
            "someone else's wicket"
        );
        assert_eq!(
            status(Mode::Link, Some(&cli), &link, None)
                .occupied_by
                .as_deref(),
            Some("a file")
        );
    }

    #[test]
    fn the_login_shells_answer_is_read_from_its_marked_lines() {
        let text = format!(
            "Welcome back!\n\n{PATH_MARK}/usr/bin:/home/me/.local/bin\n{COMMAND_MARK}/home/me/.local/bin/wicket\n"
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
            Some(PathBuf::from("/home/me/.local/bin/wicket"))
        );

        let none = parse_shell(&format!("{PATH_MARK}/usr/bin\n{COMMAND_MARK}\n")).unwrap();
        assert_eq!(none.command, None);
        assert_eq!(parse_shell("no marks at all"), None);
    }

    #[test]
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

    /// A stand-in CLI: a script that answers `--version` the way `wicket` does.
    #[cfg(unix)]
    fn script(path: &Path, version: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("#!/bin/sh\necho 'wicket {version}'\n")).unwrap();
        executable(path).unwrap();
    }

    #[test]
    fn the_mode_follows_the_clis_name_and_the_appimage() {
        assert_eq!(
            mode(
                Path::new("/Applications/Wicket.app/Contents/MacOS/wicket-cli"),
                false
            ),
            Mode::Link
        );
        assert_eq!(mode(Path::new("/usr/bin/wicket"), false), Mode::Package);
        assert_eq!(
            mode(Path::new("/tmp/.mount_WicketX/usr/bin/wicket"), true),
            Mode::Copy
        );
    }

    #[test]
    fn a_package_has_nothing_to_install() {
        let tmp = tempfile::tempdir().unwrap();
        let cli = tmp.path().join("usr/bin/wicket");
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
        let cli = tmp.path().join("mount/usr/bin/wicket");
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
        let cli = tmp.path().join("mount/usr/bin/wicket");
        script(&cli, "0.2.0");
        let link = link_path(&tmp.path().join("home"));
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::fs::write(&link, "someone else's wicket").unwrap();

        let s = status(Mode::Copy, Some(&cli), &link, None);
        assert_eq!(s.occupied_by.as_deref(), Some("a file"));
        assert!(!s.outdated);
        let err = install(Mode::Copy, &cli, &link).unwrap_err();
        assert!(err.contains("not the wicket CLI"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&link).unwrap(),
            "someone else's wicket"
        );
    }
}
