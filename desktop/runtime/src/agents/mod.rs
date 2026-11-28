//! The adapters of the agents with behaviour of their own.

mod antigravity;
mod claude_code;
mod codex;
mod codex_trust;
mod copilot;
mod cursor;
mod grok;
mod opencode;
mod pi;
#[cfg(feature = "stand-in")]
mod stand_in;

pub use antigravity::Antigravity;
pub use claude_code::ClaudeCode;
pub use codex::Codex;
pub use copilot::Copilot;
pub use cursor::{Cursor, is_cursor_command};
pub use grok::Grok;
pub use opencode::OpenCode;
pub use pi::Pi;
#[cfg(feature = "stand-in")]
pub(crate) use stand_in::SPEC as STAND_IN;
#[cfg(feature = "stand-in")]
pub use stand_in::StandIn;

/// Whether `args` are an interpreter, node or bun, running a script from
/// the package `package`: as npm's shim on Windows starts an agent, whose
/// command line keeps the interpreter's name first, a process title
/// changing only the console's title there. The script is the first
/// argument that is not one of the interpreter's own flags, such as
/// `--use-system-ca` or `--max-old-space-size=4096`, or the module a flag
/// such as `--require` names. Its path is read with either separator, in
/// any case.
pub(crate) fn runs_package_script(args: &[String], package: &str) -> bool {
    let name = |path: &str| {
        let file = path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(path)
            .to_lowercase();
        file.strip_suffix(".exe")
            .map(str::to_string)
            .unwrap_or(file)
    };
    let Some(interpreter) = args.first() else {
        return false;
    };
    if !matches!(name(interpreter).as_str(), "node" | "bun") {
        return false;
    }
    let mut rest = args[1..].iter();
    let script = loop {
        match rest.next() {
            Some(arg) if VALUE_FLAGS.contains(&arg.as_str()) => {
                rest.next();
            }
            Some(arg) if arg.starts_with('-') => {}
            Some(arg) => break arg,
            None => return false,
        }
    };
    script
        .split(['/', '\\'])
        .any(|part| part.eq_ignore_ascii_case(package))
}

/// The interpreters' flags whose value is the argument after them, when
/// not given with `=`.
const VALUE_FLAGS: &[&str] = &[
    "-r",
    "--require",
    "--import",
    "--loader",
    "--experimental-loader",
    "-C",
    "--conditions",
];

#[cfg(test)]
mod tests {
    use super::runs_package_script;

    fn runs(list: &[&str]) -> bool {
        let args: Vec<String> = list.iter().map(|a| a.to_string()).collect();
        runs_package_script(&args, "pi-coding-agent")
    }

    const SCRIPT: &str =
        r"C:\Users\me\AppData\Roaming\npm\node_modules\@mariozechner\pi-coding-agent\dist\cli.js";

    #[test]
    fn the_script_is_found_after_the_interpreter_s_own_flags() {
        assert!(runs(&[r"C:\Program Files\nodejs\node.exe", SCRIPT]));
        assert!(runs(&[
            "node.exe",
            "--use-system-ca",
            SCRIPT,
            "--session",
            "x"
        ]));
        assert!(runs(&[
            "node",
            "--no-warnings",
            "--max-old-space-size=4096",
            "/usr/lib/node_modules/pi-coding-agent/dist/cli.js",
        ]));
        assert!(runs(&["bun", "--bun", SCRIPT]));
        // a flag whose value is the next argument
        assert!(runs(&[
            "node",
            "--require",
            "./hook.js",
            "--no-warnings",
            SCRIPT
        ]));
        assert!(runs(&["node", "-r", "./hook.js", "--", SCRIPT]));
    }

    #[test]
    fn only_the_script_names_the_package_not_what_it_is_given() {
        assert!(!runs(&["node", "server.js", SCRIPT]));
        assert!(!runs(&[
            "node",
            "--no-warnings",
            "server.js",
            "--config",
            SCRIPT
        ]));
        assert!(!runs(&["node", "--no-warnings"]));
        assert!(!runs(&["node"]));
        assert!(!runs(&["python", SCRIPT]));
        assert!(!runs(&["python", "--no-warnings", SCRIPT]));
    }
}
