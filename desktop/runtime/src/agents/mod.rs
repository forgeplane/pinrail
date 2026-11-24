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
