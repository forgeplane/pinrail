//! What a run's terminal reports leave out when the agent's hook report was
//! lost, such as a Stop hook that the agent cancelled after its timeout: the
//! agent's own transcript gives the reply of the turn the terminal ended,
//! and the prompt of a turn the terminal works in.

use crate::db::{Report, ReportEvent};

use super::Runtime;

impl Runtime {
    /// Completes the terminal's `report` for the session from the agent's
    /// transcript: its turn end takes the reply of the open turn, which no
    /// hook ended. Answers the report of the prompt to record first when the
    /// terminal works in a turn no hook opened, and whether `report` now
    /// carries the agent's text.
    pub fn from_transcript(
        &self,
        agent: &str,
        session: &str,
        report: &mut Report,
    ) -> (Option<Report>, bool) {
        let Ok(Some(crate::db::LatestTurn {
            agent_turn: latest,
            ended,
            transcript: Some(path),
        })) = self.db.latest_turn(agent, session)
        else {
            return (None, false);
        };
        // the transcript is read only when a hook's report is missing: a
        // turn end while a turn is open, or work while none is
        let missing = match &report.event {
            ReportEvent::TurnEnded { end, reply } => end == "done" && reply.is_none() && !ended,
            ReportEvent::Working => ended,
            _ => false,
        };
        if !missing {
            return (None, false);
        }
        let Some(turn) = pinrail_runtime::adapter(agent)
            .and_then(|a| a.transcript_turn(std::path::Path::new(&path)))
        else {
            return (None, false);
        };
        let same = turn.agent_turn.is_some() && turn.agent_turn == latest;
        match &mut report.event {
            // the turn the hooks opened ends without the Stop hook's reply
            ReportEvent::TurnEnded { end, reply } if end == "done" && reply.is_none() => {
                if ended || !(same || latest.is_none()) {
                    return (None, false);
                }
                *reply = turn.reply;
                (None, reply.is_some())
            }
            // the agent works on a prompt whose hook never reported it
            ReportEvent::Working if ended && !same && turn.prompt.is_some() => {
                let at = turn
                    .prompted_at
                    .as_deref()
                    .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                    .map(|at| at.with_timezone(&chrono::Utc))
                    .unwrap_or(report.at);
                let prompt = Report {
                    event: ReportEvent::Prompt {
                        prompt: turn.prompt,
                    },
                    at,
                    agent_turn: turn.agent_turn,
                    ..report.clone()
                };
                (Some(prompt), false)
            }
            _ => (None, false),
        }
    }
}
