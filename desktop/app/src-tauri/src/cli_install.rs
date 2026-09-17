//! Putting the bundled CLI on the PATH. A release bundle carries the CLI as
//! `wicket-cli` beside the app's own binary; installing it links
//! `~/.local/bin/wicket` to that file, so updating the app updates the CLI
//! and nothing is copied. A development build has no bundled CLI.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde::Serialize;

/// The sidecar's name in the bundle, beside `Wicket`.
const SIDECAR: &str = "wicket-cli";
/// The name a shell runs it by.
const COMMAND: &str = "wicket";
/// How long a login shell may take to say what its PATH is.
const SHELL_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Status {
    /// the CLI inside the app bundle; none in a development build
    pub bundled: Option<PathBuf>,
    /// where the link goes
    pub link: PathBuf,
    /// the link is there and points at the bundled CLI
    pub installed: bool,
    /// something else already has the link's place: a file, or a link elsewhere
    pub occupied_by: Option<String>,
    /// what `wicket` runs in a new terminal, when the login shell said
    pub runs: Option<PathBuf>,
    /// whether the link's folder is on a new terminal's PATH; none when the
    /// login shell could not be asked
    pub dir_on_path: Option<bool>,
}

/// The CLI shipped beside the running app, when there is one.
pub fn bundled() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let sidecar = exe.parent()?.join(SIDECAR);
    sidecar.is_file().then_some(sidecar)
}

/// `~/.local/bin/wicket`.
pub fn link_path(home: &Path) -> PathBuf {
    home.join(".local").join("bin").join(COMMAND)
}

/// What is at the link's place, against the bundled CLI, and what a new
/// terminal would run. `shell` is what the login shell reported, if asked.
pub fn status(bundled: Option<&Path>, link: &Path, shell: Option<&ShellView>) -> Status {
    let (installed, occupied_by) = match std::fs::symlink_metadata(link) {
        Err(_) => (false, None),
        Ok(meta) if meta.file_type().is_symlink() => {
            let target = std::fs::read_link(link).ok();
            let ours = match (bundled, target.as_deref()) {
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
        Ok(_) => (false, Some("a file".to_string())),
    };
    let dir = link.parent().map(Path::to_path_buf);
    Status {
        bundled: bundled.map(Path::to_path_buf),
        link: link.to_path_buf(),
        installed,
        occupied_by,
        runs: shell.and_then(|s| s.command.clone()),
        dir_on_path: shell.map(|s| {
            dir.as_ref()
                .is_some_and(|d| s.path.iter().any(|p| same_file(p, d)))
        }),
    }
}

/// Links `link` to `bundled`, replacing a link that is already there. A file
/// or a folder in its place is left alone and reported.
pub fn install(bundled: &Path, link: &Path) -> Result<(), String> {
    if let Some(dir) = link.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    match std::fs::symlink_metadata(link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            std::fs::remove_file(link)
                .map_err(|e| format!("cannot replace {}: {e}", link.display()))?;
        }
        Ok(_) => {
            return Err(format!(
                "{} is a file, not a link; move it aside and install again",
                link.display()
            ));
        }
        Err(_) => {}
    }
    symlink(bundled, link).map_err(|e| format!("cannot link {}: {e}", link.display()))
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
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        let _ = tx.send(text);
    });
    let text = match rx.recv_timeout(SHELL_TIMEOUT) {
        Ok(text) => text,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
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

        assert!(!status(Some(&cli), &link, None).installed);
        install(&cli, &link).unwrap();
        let now = status(Some(&cli), &link, None);
        assert!(now.installed);
        assert_eq!(now.occupied_by, None);

        // a link left by an older copy of the app is replaced
        let old = tmp.path().join("old-wicket");
        std::fs::write(&old, "").unwrap();
        std::fs::remove_file(&link).unwrap();
        symlink(&old, &link).unwrap();
        let before = status(Some(&cli), &link, None);
        assert!(!before.installed);
        assert!(before.occupied_by.unwrap().starts_with("a link to "));
        install(&cli, &link).unwrap();
        assert!(status(Some(&cli), &link, None).installed);
    }

    #[test]
    fn a_file_in_the_links_place_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let cli = bundle(tmp.path());
        let link = link_path(&tmp.path().join("home"));
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::fs::write(&link, "someone else's wicket").unwrap();

        let err = install(&cli, &link).unwrap_err();
        assert!(err.contains("is a file, not a link"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&link).unwrap(),
            "someone else's wicket"
        );
        assert_eq!(
            status(Some(&cli), &link, None).occupied_by.as_deref(),
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
        install(&cli, &link).unwrap();

        let on = ShellView {
            path: vec![link.parent().unwrap().to_path_buf()],
            command: Some(link.clone()),
        };
        let s = status(Some(&cli), &link, Some(&on));
        assert_eq!(s.dir_on_path, Some(true));
        assert_eq!(s.runs, Some(link.clone()));

        let off = ShellView {
            path: vec![PathBuf::from("/usr/bin")],
            command: None,
        };
        assert_eq!(
            status(Some(&cli), &link, Some(&off)).dir_on_path,
            Some(false)
        );
        assert_eq!(status(Some(&cli), &link, None).dir_on_path, None);
    }
}
