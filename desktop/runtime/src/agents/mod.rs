//! The adapters of the agents with behaviour of their own.

mod antigravity;
mod claude_code;
mod codex;

pub use antigravity::Antigravity;
pub use claude_code::ClaudeCode;
pub use codex::Codex;
