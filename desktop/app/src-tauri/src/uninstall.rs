//! What the Windows uninstaller asks the app to do before it removes the
//! app: `Pinrail.exe --before-uninstall`, run by the NSIS hook in
//! `windows/hooks.nsh`. It takes back what the app put outside its own
//! folder, which the uninstaller knows nothing of:
//!
//! - the terminal host, which runs from a copy in the data folder and so
//!   does not block the uninstaller, is asked to stop, as `pinrail-host
//!   stop` does;
//! - the copy of the CLI at `~\.local\bin\pinrail.exe` is removed, if it is
//!   the pinrail CLI, by the rule the app replaces a copy by;
//! - the folder's entry in the user's PATH is removed, if Pinrail added it
//!   and nothing else is left in the folder.
//!
//! The hook does not run this for an update, so an update never ends a
//! running agent.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::cli_install;

/// The argument the uninstaller's hook starts the app with.
#[cfg_attr(not(windows), allow(dead_code))]
pub const ARG: &str = "--before-uninstall";

/// How long the host may take to stop: `pinrail-host stop` itself waits 15
/// seconds for the host to be gone.
#[cfg_attr(not(windows), allow(dead_code))]
const STOP_TIMEOUT: Duration = Duration::from_secs(30);

/// Whether the app was started by the uninstaller's hook.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn asked(args: &[String]) -> bool {
    args.iter().skip(1).any(|a| a == ARG)
}

/// Stops the host and removes the CLI and its PATH entry, each whether or
/// not the others could be. Returns the process's exit code: 1 when
/// something could not be done, which the uninstaller notes and goes on.
#[cfg(windows)]
pub fn run() -> i32 {
    let mut failures = Vec::new();
    let exe = std::env::current_exe().ok();
    if let Some(dir) = exe.as_deref().and_then(Path::parent)
        && let Err(failure) = stop_host(&dir.join("pinrail-host.exe"), STOP_TIMEOUT)
    {
        failures.push(failure);
    }
    match pinrail_core::config::home_dir() {
        Some(home) => failures.extend(remove_cli(
            cli_install::bundled().as_deref(),
            &cli_install::link_path(&home),
            &pinrail_core::Config::from_env().data_dir,
            &cli_install::remove_from_user_path,
        )),
        None => failures.push("no home folder to remove the CLI from".to_string()),
    }
    for failure in &failures {
        eprintln!("pinrail: {failure}");
    }
    i32::from(!failures.is_empty())
}

/// Runs `host stop`, which ends the running host and its terminals and
/// returns once the host is gone, or at once when none runs. It finds the
/// host by the data folder, the same way the app does. Nothing is done
/// when the app carries no host.
#[cfg_attr(not(windows), allow(dead_code))]
fn stop_host(host: &Path, timeout: Duration) -> Result<(), String> {
    if !host.is_file() {
        return Ok(());
    }
    let fail = |e: String| format!("cannot stop the terminal host: {e}");
    let mut child = cli_install::without_window(&mut Command::new(host))
        .arg("stop")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| fail(e.to_string()))?;
    let until = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(fail(format!("`{} stop`: {status}", host.display()))),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(fail(format!(
                    "`{} stop` did not finish in {} seconds",
                    host.display(),
                    timeout.as_secs()
                )));
            }
            Err(e) => return Err(fail(e.to_string())),
        }
    }
}

/// Removes the copy of the CLI at `link`, then takes the folder that
/// `data_dir` records as put on the PATH by Pinrail off it again, with
/// `off_path`, and removes the folder. The PATH entry stays while anything
/// else is in the folder: another program's commands there are run through
/// it, whoever added it. Returns what could not be done.
#[cfg_attr(not(windows), allow(dead_code))]
fn remove_cli(
    bundled: Option<&Path>,
    link: &Path,
    data_dir: &Path,
    off_path: &dyn Fn(&Path) -> Result<bool, String>,
) -> Vec<String> {
    let mut failures = Vec::new();
    if let Err(failure) = cli_install::remove_copy(bundled, link) {
        failures.push(failure);
    }
    if let Some(dir) = cli_install::recorded_path_entry(data_dir)
        && nothing_in(&dir)
    {
        match off_path(&dir) {
            Ok(_) => {
                cli_install::forget_path_entry(data_dir);
                let _ = std::fs::remove_dir(&dir);
            }
            Err(failure) => failures.push(failure),
        }
    }
    failures
}

/// Whether the folder is empty, or not there.
#[cfg_attr(not(windows), allow(dead_code))]
fn nothing_in(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(mut entries) => entries.next().is_none(),
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::cell::RefCell;
    #[cfg(unix)]
    use std::path::PathBuf;

    fn manifest_file(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    /// The hook and the app agree on the argument, the hook leaves an
    /// update alone, and the release build's installer carries the hook.
    #[test]
    fn the_uninstallers_hook_starts_the_app_with_the_argument_unless_updating() {
        let hook = manifest_file("windows/hooks.nsh");
        let code: Vec<&str> = hook
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with(';'))
            .collect();
        assert_eq!(code.first(), Some(&"!macro NSIS_HOOK_PREUNINSTALL"));
        assert_eq!(code.last(), Some(&"!macroend"));
        // everything the hook does is inside the check for an update
        assert_eq!(code[1], "${If} $UpdateMode <> 1");
        assert_eq!(code[code.len() - 2], "${EndIf}");
        let run = format!(r#"ExecWait '"$INSTDIR\${{MAINBINARYNAME}}.exe" {ARG}' $0"#);
        assert!(code.contains(&run.as_str()), "{hook}");

        let config: serde_json::Value =
            serde_json::from_str(&manifest_file("tauri.release.windows.conf.json")).unwrap();
        assert_eq!(
            config["bundle"]["windows"]["nsis"]["installerHooks"],
            "./windows/hooks.nsh"
        );
    }

    #[test]
    fn only_the_hooks_argument_asks_for_it() {
        let args = |list: &[&str]| list.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        assert!(asked(&args(&["Pinrail.exe", "--before-uninstall"])));
        assert!(!asked(&args(&["Pinrail.exe"])));
        assert!(!asked(&args(&["--before-uninstall"])));
        assert!(!asked(&args(&["Pinrail.exe", "pinrail://open"])));
    }

    /// A stand-in program: a script that answers as `body` says.
    #[cfg(unix)]
    fn script(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// What `remove_cli` took off the PATH, and its failures.
    #[cfg(unix)]
    fn removed(
        bundled: &Path,
        link: &Path,
        data_dir: &Path,
        result: Result<bool, String>,
    ) -> (Vec<PathBuf>, Vec<String>) {
        let taken = RefCell::new(Vec::new());
        let failures = remove_cli(Some(bundled), link, data_dir, &|dir| {
            taken.borrow_mut().push(dir.to_path_buf());
            result.clone()
        });
        (taken.into_inner(), failures)
    }

    #[cfg(unix)]
    #[test]
    fn the_copy_goes_and_the_path_entry_pinrail_added_with_it() {
        let tmp = tempfile::tempdir().unwrap();
        let bundled = tmp.path().join("app/pinrail");
        script(&bundled, "echo 'pinrail 0.2.0'");
        let link = cli_install::link_path(&tmp.path().join("home"));
        let bin = link.parent().unwrap().to_path_buf();
        let data_dir = tmp.path().join("data");

        // installed, but the folder was on the PATH already: not Pinrail's
        cli_install::install(cli_install::Mode::Copy, &bundled, &link).unwrap();
        let (taken, failures) = removed(&bundled, &link, &data_dir, Ok(true));
        assert!(!link.exists());
        assert!(bin.is_dir(), "a folder Pinrail has no record of stays");
        assert_eq!((taken, failures), (vec![], vec![]));

        // installed, and Pinrail put the folder on the PATH
        cli_install::install(cli_install::Mode::Copy, &bundled, &link).unwrap();
        cli_install::record_path_entry(&data_dir, &bin).unwrap();
        let (taken, failures) = removed(&bundled, &link, &data_dir, Ok(true));
        assert_eq!((taken, failures), (vec![bin.clone()], vec![]));
        assert!(!bin.exists());
        assert_eq!(cli_install::recorded_path_entry(&data_dir), None);
    }

    #[cfg(unix)]
    #[test]
    fn the_path_entry_stays_while_something_else_is_in_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let bundled = tmp.path().join("app/pinrail");
        script(&bundled, "echo 'pinrail 0.2.0'");
        let link = cli_install::link_path(&tmp.path().join("home"));
        let bin = link.parent().unwrap().to_path_buf();
        let data_dir = tmp.path().join("data");
        cli_install::install(cli_install::Mode::Copy, &bundled, &link).unwrap();
        cli_install::record_path_entry(&data_dir, &bin).unwrap();
        script(&bin.join("other"), "echo other");

        let (taken, failures) = removed(&bundled, &link, &data_dir, Ok(true));
        assert_eq!((taken, failures), (vec![], vec![]));
        assert!(!link.exists() && bin.join("other").is_file());
        assert_eq!(cli_install::recorded_path_entry(&data_dir), Some(bin));
    }

    #[cfg(unix)]
    #[test]
    fn a_path_that_cannot_be_changed_is_reported_and_stays_recorded() {
        let tmp = tempfile::tempdir().unwrap();
        let bundled = tmp.path().join("app/pinrail");
        script(&bundled, "echo 'pinrail 0.2.0'");
        let link = cli_install::link_path(&tmp.path().join("home"));
        let bin = link.parent().unwrap().to_path_buf();
        let data_dir = tmp.path().join("data");
        std::fs::create_dir_all(&bin).unwrap();
        cli_install::record_path_entry(&data_dir, &bin).unwrap();

        let (taken, failures) = removed(&bundled, &link, &data_dir, Err("no".to_string()));
        assert_eq!(
            (taken, failures),
            (vec![bin.clone()], vec!["no".to_string()])
        );
        assert!(bin.is_dir());
        assert_eq!(cli_install::recorded_path_entry(&data_dir), Some(bin));
    }

    #[cfg(unix)]
    #[test]
    fn the_host_is_asked_to_stop_and_a_failure_or_a_hang_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let host = tmp.path().join("pinrail-host");
        let long = Duration::from_secs(20);
        assert_eq!(stop_host(&host, long), Ok(()), "no host in the app");

        let asked = tmp.path().join("asked");
        script(&host, &format!("echo \"$1\" > '{}'", asked.display()));
        assert_eq!(stop_host(&host, long), Ok(()));
        assert_eq!(std::fs::read_to_string(&asked).unwrap(), "stop\n");

        script(&host, "exit 1");
        assert!(stop_host(&host, long).unwrap_err().contains("stop`: "));

        script(&host, "exec sleep 20");
        let started = Instant::now();
        let failure = stop_host(&host, Duration::from_millis(300)).unwrap_err();
        assert!(failure.contains("did not finish"), "{failure}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
