//! What an agent's hooks tell Pinrail about its session: that it started,
//! works on a turn, waits for the person, ended a turn or ended. A report
//! never carries a command, only what the agent's own input said.

/// One hook, as Pinrail reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookReport {
    /// The id of the session the hook fired in.
    pub session: String,
    pub event: HookEvent,
    pub detail: Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    /// The session started, or was resumed, cleared, compacted or forked.
    Started { source: StartSource },
    /// The person sent a prompt: a turn begins.
    Prompt,
    /// The agent works on the turn, such as running a tool. It also ends a
    /// wait for the person, once they answered.
    Working,
    /// The agent waits for the person.
    NeedsInput { kind: WaitKind },
    /// The turn ended, and the agent waits for the next prompt.
    TurnEnded { end: TurnEnd },
    /// The session ended.
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartSource {
    Startup,
    Resume,
    Clear,
    Compact,
    Fork,
}

/// What the agent waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitKind {
    /// The person's approval of a tool call.
    Permission,
    /// An answer to a question the agent asked.
    Question,
}

/// How a turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnEnd {
    Done,
    Failed,
    Interrupted,
}

/// What the hook's input says beside the event, each when it says it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Detail {
    /// The tool a permission is asked for.
    pub tool: Option<String>,
    /// A short description of the tool call, such as its command.
    pub summary: Option<String>,
    /// The person's prompt that begins a turn.
    pub prompt: Option<String>,
    /// The agent's last message of a turn.
    pub reply: Option<String>,
    /// Why a turn failed.
    pub error: Option<String>,
    /// Why the session ended.
    pub reason: Option<String>,
    pub transcript_path: Option<String>,
    pub cwd: Option<String>,
}

/// The longest summary of a tool call, in characters.
const SUMMARY_CHARS: usize = 200;

/// A string field of the hook's input, when it is a non-empty string.
pub(crate) fn text(input: &serde_json::Value, key: &str) -> Option<String> {
    input
        .get(key)?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// A tool call in a few words: its command, its file or its description,
/// on one line and at most [`SUMMARY_CHARS`] characters.
pub(crate) fn summary(tool_input: Option<&serde_json::Value>) -> Option<String> {
    let input = tool_input?;
    let said = ["command", "file_path", "path", "url", "description"]
        .iter()
        .find_map(|key| text(input, key))?;
    let line = said.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(line.chars().take(SUMMARY_CHARS).collect())
}

/// What every report carries from the input: the cwd and the transcript.
pub(crate) fn common(input: &serde_json::Value) -> Detail {
    Detail {
        transcript_path: text(input, "transcript_path"),
        cwd: text(input, "cwd"),
        ..Detail::default()
    }
}

/// The start source a `SessionStart` names, among the five both agents use.
pub(crate) fn start_source(input: &serde_json::Value) -> Option<StartSource> {
    Some(match input.get("source")?.as_str()? {
        "startup" => StartSource::Startup,
        "resume" => StartSource::Resume,
        "clear" => StartSource::Clear,
        "compact" => StartSource::Compact,
        "fork" => StartSource::Fork,
        _ => return None,
    })
}
