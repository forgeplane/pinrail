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

impl WaitKind {
    pub fn as_str(self) -> &'static str {
        match self {
            WaitKind::Permission => "permission",
            WaitKind::Question => "question",
        }
    }
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
    /// The agent's own transcript of the session, which holds the whole
    /// conversation.
    pub transcript_path: Option<String>,
    /// The agent's own id for the turn, which places it in the transcript:
    /// Claude Code's `prompt_id`, Codex's `turn_id`.
    pub turn_id: Option<String>,
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
        turn_id: text(input, "turn_id").or_else(|| text(input, "prompt_id")),
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

/// The longest prompt and reply a report carries, in characters; the core
/// keeps no more. They are excerpts: the whole conversation is in the
/// agent's transcript. A prompt only has to say what the turn was about;
/// a reply keeps its end, which is often the conclusion, up to this much.
pub const PROMPT_CHARS: usize = 4_000;
pub const REPLY_CHARS: usize = 32_000;

impl StartSource {
    pub fn as_str(self) -> &'static str {
        match self {
            StartSource::Startup => "startup",
            StartSource::Resume => "resume",
            StartSource::Clear => "clear",
            StartSource::Compact => "compact",
            StartSource::Fork => "fork",
        }
    }
}

impl HookReport {
    /// The report as `POST /api/v1/sessions/{agent}/{id}/reports` takes it,
    /// without the time, the process and the origin, which the caller adds.
    /// A failed turn's error stands for its reply, and every text is cut to
    /// what the core keeps, with `prompt_cut` or `reply_cut` when it was.
    pub fn to_body(&self) -> serde_json::Value {
        use serde_json::{Map, Value, json};
        let cut = |text: &Option<String>, n: usize| -> Option<String> {
            text.as_ref().map(|t| t.chars().take(n).collect())
        };
        let mut body = Map::new();
        let event = match self.event {
            HookEvent::Started { source } => {
                body.insert("source".into(), json!(source.as_str()));
                "started"
            }
            HookEvent::Prompt => "prompt",
            HookEvent::Working => "working",
            HookEvent::NeedsInput { kind } => {
                body.insert("wait".into(), json!(kind.as_str()));
                "needs_input"
            }
            HookEvent::TurnEnded { end } => {
                let end = match end {
                    TurnEnd::Done => "done",
                    TurnEnd::Failed => "failed",
                    TurnEnd::Interrupted => "interrupted",
                };
                body.insert("end".into(), json!(end));
                "turn_ended"
            }
            HookEvent::Ended => "ended",
        };
        body.insert("event".into(), json!(event));
        let d = &self.detail;
        let reply = d.reply.clone().or_else(|| d.error.clone());
        let longer = |text: &Option<String>, n: usize| {
            text.as_ref().is_some_and(|t| t.chars().nth(n).is_some())
        };
        let mut detail: Map<String, Value> = [
            ("tool", cut(&d.tool, SUMMARY_CHARS)),
            ("summary", cut(&d.summary, SUMMARY_CHARS)),
            ("prompt", cut(&d.prompt, PROMPT_CHARS)),
            ("reply", cut(&reply, REPLY_CHARS)),
            ("reason", d.reason.clone()),
            ("turn_id", d.turn_id.clone()),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k.to_string(), json!(v))))
        .collect();
        if longer(&d.prompt, PROMPT_CHARS) {
            detail.insert("prompt_cut".into(), json!(true));
        }
        if longer(&reply, REPLY_CHARS) {
            detail.insert("reply_cut".into(), json!(true));
        }
        if let Some(path) = &d.transcript_path {
            body.insert("transcript_path".into(), json!(path));
        }
        if !detail.is_empty() {
            body.insert("detail".into(), Value::Object(detail));
        }
        Value::Object(body)
    }
}
