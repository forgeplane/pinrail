//! The adapters of the agents with behaviour of their own.

mod antigravity;
mod claude_code;
mod codex;
mod copilot;
mod cursor;
mod grok;
mod opencode;
mod pi;

pub use antigravity::Antigravity;
pub use claude_code::ClaudeCode;
pub use codex::Codex;
pub use copilot::Copilot;
pub use cursor::{Cursor, is_cursor_command};
pub use grok::Grok;
pub use opencode::OpenCode;
pub use pi::Pi;
