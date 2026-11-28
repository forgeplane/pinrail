//! The environment an agent starts with: a base, without what would tie it
//! to another session or terminal, with the terminal's and Pinrail's
//! variables, the desktop session's variables the core sends, and the
//! request's own, which win.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The desktop session's variables. The host outlives logins, so it never
/// takes these from its own environment; the core sends them with each
/// spawn, and one it does not send is removed.
const SESSION_VARIABLES: &[&str] = &[
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "DBUS_SESSION_BUS_ADDRESS",
    "SSH_AUTH_SOCK",
];

fn is_session_variable(name: &str) -> bool {
    SESSION_VARIABLES.contains(&name) || name.starts_with("XDG_SESSION_")
}

/// What a spawn asks for.
pub struct Request<'a> {
    /// The base: the probed login environment, or the host's own.
    pub base: &'a [(String, String)],
    pub env: &'a BTreeMap<String, String>,
    pub session_env: &'a BTreeMap<String, String>,
    /// The folder as the request spells it, symbolic links unresolved.
    pub cwd: &'a str,
    pub terminal: &'a str,
}

/// The agent's environment.
pub fn build(request: &Request) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = request
        .base
        .iter()
        .filter(|(name, _)| {
            !pinrail_runtime::scrub::is_scrubbed(name)
                && !is_session_variable(name)
                && name != "OLDPWD"
        })
        .cloned()
        .collect();
    let mut set = |name: &str, value: &str| set_var(&mut env, name, value);
    set("TERM", "xterm-256color");
    set("COLORTERM", "truecolor");
    set("TERM_PROGRAM", "Pinrail");
    set("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
    set("PINRAIL_TERMINAL_ID", request.terminal);
    set("PWD", request.cwd);
    if !env.contains_key("LANG") && !env.contains_key("LC_ALL") {
        env.insert("LANG".to_string(), "en_US.UTF-8".to_string());
    }
    for (name, value) in request.session_env {
        if is_session_variable(name) {
            set_var(&mut env, name, value);
        }
    }
    if let Some(cli) = find_program("pinrail", var(&env, "PATH")) {
        set_var(&mut env, "PINRAIL_CLI", &cli.to_string_lossy());
    }
    for (name, value) in request.env {
        set_var(&mut env, name, value);
    }
    env
}

/// Sets a variable. Windows takes names without regard to case, so there
/// the one it replaces may be spelt otherwise, as `Path` for `PATH`.
fn set_var(env: &mut BTreeMap<String, String>, name: &str, value: &str) {
    if cfg!(windows) {
        env.retain(|key, _| !key.eq_ignore_ascii_case(name));
    }
    env.insert(name.to_string(), value.to_string());
}

/// A variable's value, its name spelt otherwise on Windows.
pub fn var<'a>(env: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
    if cfg!(windows) {
        return env
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str());
    }
    env.get(name).map(String::as_str)
}

/// A program's path, found as Windows finds it: a name with a folder in it
/// as it is, otherwise the first of that name on `path`, which separates
/// its folders with `;`, as it is or with an extension from `PATHEXT`.
#[cfg(windows)]
pub fn find_program(name: &str, path: Option<&str>) -> Option<PathBuf> {
    find_program_in(name, path, None, None)
}

/// [`find_program`] for a program a terminal starts: with the terminal's
/// own `PATHEXT`, and a relative path such as `.\tool.exe` from the
/// terminal's folder, not this process's.
#[cfg(windows)]
pub fn find_program_in(
    name: &str,
    path: Option<&str>,
    pathext: Option<&str>,
    cwd: Option<&Path>,
) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let exts = pathext
        .map(str::to_string)
        .or_else(|| std::env::var("PATHEXT").ok())
        .filter(|e| !e.is_empty())
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_string());
    let found = |base: PathBuf| -> Option<PathBuf> {
        if base.extension().is_some() && base.is_file() {
            return Some(base);
        }
        // in lower case, as programs' files are named: PATHEXT spells them
        // in capitals, and the file is found either way
        exts.split(';')
            .filter(|e| !e.is_empty())
            .map(|e| PathBuf::from(format!("{}{}", base.display(), e.to_lowercase())))
            .find(|p| p.is_file())
    };
    let program = Path::new(name);
    if program.components().count() > 1 || program.is_absolute() {
        let program = match cwd {
            Some(cwd) if program.is_relative() => cwd.join(program),
            _ => program.to_path_buf(),
        };
        return found(program);
    }
    path?
        .split(';')
        .filter(|dir| !dir.is_empty())
        .find_map(|dir| found(Path::new(dir).join(name)))
}

/// A program's path, found as a shell would: a name with a slash as it
/// is, otherwise the first executable file of that name on `path`.
#[cfg(unix)]
pub fn find_program(name: &str, path: Option<&str>) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    if name.contains('/') {
        return Some(PathBuf::from(name));
    }
    path?
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join(name))
        .find(|candidate| executable(candidate))
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(vars: &[(&str, &str)]) -> Vec<(String, String)> {
        vars.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn map(vars: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs(vars).into_iter().collect()
    }

    fn build_with(
        base: &[(&str, &str)],
        env: &[(&str, &str)],
        session: &[(&str, &str)],
    ) -> BTreeMap<String, String> {
        build(&Request {
            base: &pairs(base),
            env: &map(env),
            session_env: &map(session),
            cwd: "/Users/p/code/link-to-app",
            terminal: "t_1",
        })
    }

    #[test]
    fn another_session_s_variables_are_removed_and_the_terminal_s_set() {
        let env = build_with(
            &[
                ("PATH", "/usr/bin"),
                ("CLAUDECODE", "1"),
                ("CODEX_THREAD_ID", "x"),
                ("TMUX", "/tmp/tmux"),
                ("PINRAIL_RUN_TOKEN", "old"),
                ("TERM", "screen"),
                ("PWD", "/somewhere/else"),
                ("OLDPWD", "/before"),
            ],
            &[],
            &[],
        );
        for gone in [
            "CLAUDECODE",
            "CODEX_THREAD_ID",
            "TMUX",
            "PINRAIL_RUN_TOKEN",
            "OLDPWD",
        ] {
            assert!(!env.contains_key(gone), "{gone}");
        }
        assert_eq!(env["TERM"], "xterm-256color");
        assert_eq!(env["COLORTERM"], "truecolor");
        assert_eq!(env["TERM_PROGRAM"], "Pinrail");
        assert_eq!(env["PINRAIL_TERMINAL_ID"], "t_1");
        // as the request spells it
        assert_eq!(env["PWD"], "/Users/p/code/link-to-app");
        assert_eq!(env["PATH"], "/usr/bin");
    }

    #[test]
    fn the_app_s_launchd_and_bundle_variables_are_removed() {
        // as macOS sets them for an app opened from Finder; with them,
        // Claude Code cannot read its login from the keychain
        let env = build_with(
            &[
                ("PATH", "/usr/bin"),
                ("XPC_FLAGS", "0x2"),
                ("XPC_SERVICE_NAME", "0"),
                ("__CFBundleIdentifier", "dev.pinrail.desktop"),
                ("__CF_USER_TEXT_ENCODING", "0x1F5:0x0:0x0"),
            ],
            &[],
            &[],
        );
        for gone in ["XPC_FLAGS", "XPC_SERVICE_NAME", "__CFBundleIdentifier"] {
            assert!(!env.contains_key(gone), "{gone}");
        }
        assert_eq!(env["__CF_USER_TEXT_ENCODING"], "0x1F5:0x0:0x0");
    }

    #[test]
    fn lang_is_set_only_when_no_locale_is() {
        assert_eq!(build_with(&[], &[], &[])["LANG"], "en_US.UTF-8");
        assert_eq!(
            build_with(&[("LANG", "el_GR.UTF-8")], &[], &[])["LANG"],
            "el_GR.UTF-8"
        );
        assert!(!build_with(&[("LC_ALL", "C")], &[], &[]).contains_key("LANG"));
    }

    #[test]
    fn session_variables_come_from_the_request_only() {
        let env = build_with(
            &[
                ("SSH_AUTH_SOCK", "/stale/agent"),
                ("DISPLAY", ":9"),
                ("XDG_SESSION_ID", "3"),
            ],
            &[],
            &[
                ("SSH_AUTH_SOCK", "/current/agent"),
                ("XDG_SESSION_TYPE", "wayland"),
                ("HOME", "/not/a/session/variable"),
            ],
        );
        assert_eq!(env["SSH_AUTH_SOCK"], "/current/agent");
        assert_eq!(env["XDG_SESSION_TYPE"], "wayland");
        assert!(!env.contains_key("DISPLAY"));
        assert!(!env.contains_key("XDG_SESSION_ID"));
        assert!(!env.contains_key("HOME"));
    }

    #[test]
    fn the_request_s_variables_win() {
        let env = build_with(
            &[("PATH", "/usr/bin")],
            &[("PINRAIL_RUN_ID", "run_1"), ("TERM", "dumb")],
            &[],
        );
        assert_eq!(env["PINRAIL_RUN_ID"], "run_1");
        assert_eq!(env["TERM"], "dumb");
    }

    #[cfg(windows)]
    #[test]
    fn the_pinrail_command_is_found_on_the_agent_s_path() {
        let dir = tempfile::tempdir().unwrap();
        let cli = dir.path().join("pinrail.exe");
        std::fs::write(&cli, "").unwrap();
        let path = format!(r"C:\nonexistent;{}", dir.path().display());
        // Windows' own spelling of the variable
        let env = build_with(&[("Path", &path)], &[], &[]);
        assert_eq!(env["PINRAIL_CLI"], cli.to_str().unwrap());
        assert!(!build_with(&[("Path", r"C:\nonexistent")], &[], &[]).contains_key("PINRAIL_CLI"));
        // a variable the request spells otherwise replaces it
        let env = build_with(&[("Path", &path)], &[("PATH", "C:\\x")], &[]);
        assert_eq!(var(&env, "path"), Some("C:\\x"));
        assert_eq!(
            env.keys()
                .filter(|k| k.eq_ignore_ascii_case("path"))
                .count(),
            1
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_pinrail_command_is_found_on_the_agent_s_path() {
        let dir = tempfile::tempdir().unwrap();
        let cli = dir.path().join("pinrail");
        std::fs::write(&cli, "#!/bin/sh\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!("/nonexistent:{}", dir.path().display());
        let env = build_with(&[("PATH", &path)], &[], &[]);
        assert_eq!(env["PINRAIL_CLI"], cli.to_str().unwrap());
        assert!(!build_with(&[("PATH", "/nonexistent")], &[], &[]).contains_key("PINRAIL_CLI"));
    }
}
