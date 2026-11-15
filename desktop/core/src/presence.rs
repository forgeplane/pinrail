//! What an agent is doing in its session, from the evidence its hooks
//! report: the state a report moves the session to, and the attention the
//! session asks of the person. The storage keeps the state;
//! this module decides it.

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};

use crate::db::{Next, PresenceRow, ReportEvent, StoredState};

/// How long a session may report nothing while it works before its state
/// is no longer known.
pub const STALE_MINUTES: i64 = 30;

/// Whether a process still runs: its pid, and the start time recorded for
/// it, read the same way.
pub fn alive(pid: i64, started: &str) -> bool {
    u32::try_from(pid)
        .ok()
        .and_then(pinrail_runtime::process::process)
        .is_some_and(|p| p.started == started)
}

/// The state `event` moves the session to from `stored`, or `None` when the
/// report changes nothing: an `ended` report from another process than the
/// session's, while that one still runs, since two processes can hold one
/// Claude Code session. `alive` says whether a recorded process still runs.
pub fn next(
    event: &ReportEvent,
    stored: Option<&StoredState>,
    process: Option<&(i64, String)>,
    alive: &dyn Fn(i64, &str) -> bool,
) -> Option<Next> {
    let plain = |state: &str| Next {
        state: state.to_string(),
        wait: None,
        stop_reason: None,
    };
    Some(match event {
        // a start says the session is there, and idle only when nothing
        // else is known: Claude Code also fires it after compacting in the
        // middle of a turn
        ReportEvent::Started { .. } => match stored {
            Some(s) if s.state != "stopped" && !restarted(s, process, alive) => Next {
                state: s.state.clone(),
                wait: s
                    .wait_kind
                    .clone()
                    .map(|k| (k, s.wait_tool.clone(), s.wait_summary.clone())),
                stop_reason: s.stop_reason.clone(),
            },
            _ => plain("idle"),
        },
        ReportEvent::TurnEnded { .. } => plain("idle"),
        ReportEvent::Prompt { .. } | ReportEvent::Working => plain("working"),
        ReportEvent::NeedsInput {
            kind,
            tool,
            summary,
        } => {
            // the same wait keeps what an earlier report said of it:
            // Claude Code's notification follows its permission request
            // without the tool. A report that names the call names all of it.
            let detail = match (tool, summary) {
                (None, None) => stored
                    .filter(|s| s.state == "needs_input" && s.wait_kind.as_deref() == Some(kind))
                    .map(|s| (s.wait_tool.clone(), s.wait_summary.clone()))
                    .unwrap_or((None, None)),
                _ => (tool.clone(), summary.clone()),
            };
            Next {
                state: "needs_input".to_string(),
                wait: Some((kind.clone(), detail.0, detail.1)),
                stop_reason: None,
            }
        }
        ReportEvent::Ended { reason } => {
            let other = match (stored.and_then(StoredState::process), process) {
                (Some((pid, started)), Some((own, own_started))) => {
                    (pid != *own || started != own_started.as_str()) && alive(pid, started)
                }
                _ => false,
            };
            if other {
                return None;
            }
            Next {
                state: "stopped".to_string(),
                wait: None,
                // Claude Code ends the session it clears with `clear`
                stop_reason: Some(match reason.as_deref() {
                    Some("clear") => "cleared".to_string(),
                    // the core found the agent's process gone
                    Some("exited") => "exited".to_string(),
                    // the computer restarted
                    Some("restart") => "restart".to_string(),
                    _ => "ended".to_string(),
                }),
            }
        }
    })
}

/// Whether the report comes from a new process of the agent while the one
/// stored for the session is gone: the session was resumed after its agent
/// was stopped without saying so, so what it was doing no longer holds.
fn restarted(
    stored: &StoredState,
    process: Option<&(i64, String)>,
    alive: &dyn Fn(i64, &str) -> bool,
) -> bool {
    match (stored.process(), process) {
        (Some((pid, started)), Some((own, own_started))) => {
            (pid != *own || started != own_started.as_str()) && !alive(pid, started)
        }
        _ => false,
    }
}

/// The attention a session asks of the person: whether it needs them, has
/// finished since they last looked, works, idles, stopped, or is not known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attention {
    NeedsYou,
    Finished,
    Working,
    Idle,
    Stopped,
    Unknown,
}

impl Attention {
    pub fn as_str(self) -> &'static str {
        match self {
            Attention::NeedsYou => "needs_you",
            Attention::Finished => "finished",
            Attention::Working => "working",
            Attention::Idle => "idle",
            Attention::Stopped => "stopped",
            Attention::Unknown => "unknown",
        }
    }

    pub fn parse(text: &str) -> Option<Attention> {
        [
            Attention::NeedsYou,
            Attention::Finished,
            Attention::Working,
            Attention::Idle,
            Attention::Stopped,
            Attention::Unknown,
        ]
        .into_iter()
        .find(|a| a.as_str() == text)
    }
}

/// The attention of a session with `presence`, at `now`: a question or a
/// review waiting on the person (`waiting`) asks for them whatever the
/// agent does.
pub fn attention(presence: Option<&PresenceRow>, waiting: bool, now: DateTime<Utc>) -> Attention {
    if waiting {
        return Attention::NeedsYou;
    }
    let Some(p) = presence else {
        return Attention::Unknown;
    };
    let unseen = p
        .finished_at
        .is_some_and(|finished| p.seen_at.is_none_or(|seen| seen < finished));
    match p.state.as_str() {
        "needs_input" => Attention::NeedsYou,
        "working" if now - p.updated_at > Duration::minutes(STALE_MINUTES) => Attention::Unknown,
        "working" => Attention::Working,
        "idle" if unseen => Attention::Finished,
        "idle" => Attention::Idle,
        "stopped" if unseen && p.stop_reason.as_deref() == Some("exited") => Attention::Finished,
        "stopped" => Attention::Stopped,
        _ => Attention::Unknown,
    }
}

/// The presence as the API and the events give it.
pub fn to_json(presence: &PresenceRow, attention: Attention) -> Value {
    let iso = crate::reviews::iso;
    json!({
        "state": presence.state,
        "wait": presence.wait_kind.as_ref().map(|kind| json!({
            "kind": kind,
            "tool": presence.wait_tool,
            "summary": presence.wait_summary,
        })),
        "stop_reason": presence.stop_reason,
        "since": iso(presence.state_since),
        "turn": presence.turn,
        "turn_started": presence.turn_started.map(iso),
        "finished_at": presence.finished_at.map(iso),
        "seen_at": presence.seen_at.map(iso),
        // the last report: a session that works and reported nothing for 30
        // minutes is no longer known, which the window works out from this
        "updated_at": iso(presence.updated_at),
        "attention": attention.as_str(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(state: &str, process: Option<(i64, &str)>) -> StoredState {
        StoredState {
            state: state.to_string(),
            wait_kind: None,
            wait_tool: None,
            wait_summary: None,
            stop_reason: None,
            pid: process.map(|p| p.0),
            proc_started: process.map(|p| p.1.to_string()),
        }
    }

    fn ended() -> ReportEvent {
        ReportEvent::Ended { reason: None }
    }

    #[test]
    fn a_start_from_a_new_process_after_the_old_one_died_is_idle() {
        let session = stored("working", Some((10, "a")));
        let started = ReportEvent::Started { source: None };
        let new = (20, "b".to_string());
        let gone = |_: i64, _: &str| false;
        let runs = |_: i64, _: &str| true;
        let state = |alive: &dyn Fn(i64, &str) -> bool| {
            next(&started, Some(&session), Some(&new), alive).map(|n| n.state)
        };
        assert_eq!(state(&gone), Some("idle".to_string()));
        // while the old process runs, a start is a compaction or a second
        // window: the state holds
        assert_eq!(state(&runs), Some("working".to_string()));
    }

    #[test]
    fn an_end_from_another_process_that_still_runs_changes_nothing() {
        let session = stored("working", Some((10, "a")));
        let other = (20, "b".to_string());
        let own = (10, "a".to_string());
        let runs = |_: i64, _: &str| true;
        let gone = |_: i64, _: &str| false;
        assert!(next(&ended(), Some(&session), Some(&other), &runs).is_none());
        // once the session's own process is gone, any end stops it
        assert_eq!(
            next(&ended(), Some(&session), Some(&other), &gone).map(|n| n.state),
            Some("stopped".to_string())
        );
        // its own end, and an end with no process said, stop it
        assert!(next(&ended(), Some(&session), Some(&own), &runs).is_some());
        assert!(next(&ended(), Some(&session), None, &runs).is_some());
        // a pid used again by a later process is another process
        let reused = (10, "z".to_string());
        assert!(next(&ended(), Some(&session), Some(&reused), &runs).is_none());
    }
}
