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
/// changing only the console's title there. The script's path is read with
/// either separator, in any case.
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
    let (Some(interpreter), Some(script)) = (args.first(), args.get(1)) else {
        return false;
    };
    matches!(name(interpreter).as_str(), "node" | "bun")
        && script
            .split(['/', '\\'])
            .any(|part| part.eq_ignore_ascii_case(package))
}
