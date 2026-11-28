//! What Pinrail knows about each coding agent, shared by the app and the
//! CLI: how to tell which agent a command runs under, and where each agent
//! keeps its configuration and skills.

pub mod agent;
mod agents;
pub mod checkout;
pub mod connection;
pub mod detect;
pub mod hooks;
pub mod integration;
pub mod process;
pub mod registry;
pub mod scrub;
pub mod skills;

pub use agent::{
    Agent, Env, Hint, HintStatus, StartPlan, StartRequest, SubmitPlan, TranscriptTurn, adapter,
    connector, reserved_arg, runs_as, skips_checks, split_args, start_plan, with_read_folder,
};
pub use agents::is_cursor_command;
pub use registry::{AgentSpec, Folders, all, find};
