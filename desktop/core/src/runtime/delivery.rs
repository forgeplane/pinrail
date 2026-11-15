//! Messages to an agent Pinrail runs, typed into its terminal (spec 7).
//! A message to a session with a live run whose agent has a typing plan
//! goes by the Type route; any other goes by the Queue, to the agent's
//! next status. A message to type waits while the agent works or waits for
//! an answer, and is typed once it is idle, one at a time; the agent's
//! prompt hook confirms it. A message typed is typed once by itself: after
//! that, the person retries it. One held for 30 minutes, or whose run
//! ended, goes to the Queue.

use chrono::{DateTime, Utc};

use crate::db::{MessageChange, MessageRow, RunFilters, RunRow, message_notice};
use crate::events::{Notice, SESSION_MESSAGE_STATE};

use super::runs::{LOCAL, RunError};

/// How long a typed message waits for its prompt hook before it shows as
/// unconfirmed.
pub const CONFIRM_WITHIN: chrono::Duration = chrono::Duration::seconds(10);
/// How long a message to type waits for the agent before it goes to the
/// Queue.
pub const HOLD_FOR: chrono::Duration = chrono::Duration::minutes(30);

/// Whether the agent received a typed message, from the prompt its hook
/// reported: the same text once line endings are normalised and the ends
/// trimmed, or a longer prompt that starts with it, as when the person
/// added to it in the terminal. A prompt the CLI cut to its excerpt
/// confirms a message that starts with it.
pub fn confirms(prompt: &str, typed: &str, cut: bool) -> bool {
    let normal = |s: &str| {
        s.replace("\r\n", "\n")
            .replace('\r', "\n")
            .trim()
            .to_string()
    };
    let (prompt, typed) = (normal(prompt), normal(typed));
    !typed.is_empty() && (prompt.starts_with(&typed) || (cut && typed.starts_with(&prompt)))
}

impl super::Runtime {
    /// The session's live run, if its agent has a plan for typing a message.
    fn typing_run(&self, agent: &str, session: &str) -> Result<Option<RunRow>, RunError> {
        let key = (agent.to_string(), session.to_string());
        let typed = pinrail_runtime::adapter(agent).is_some_and(|a| a.submit().is_some());
        if !typed {
            return Ok(None);
        }
        Ok(self
            .db
            .runs(&RunFilters {
                live: Some(true),
                host: Some(LOCAL.to_string()),
                ..RunFilters::default()
            })?
            .into_iter()
            .find(|r| r.session.as_ref() == Some(&key) && r.terminal.is_some()))
    }

    /// The route a new message to the session goes by: `type` while Pinrail
    /// runs it and can type into its agent, `queue` otherwise.
    pub fn message_route(&self, agent: &str, session: &str) -> &'static str {
        match self.typing_run(agent, session) {
            Ok(Some(_)) => "type",
            _ => "queue",
        }
    }

    /// Moves a message to another state and announces it; `None` when it
    /// was not in one of `from`.
    fn move_message(
        &self,
        agent: &str,
        session: &str,
        message: i64,
        from: &[&str],
        to: MessageChange<'_>,
    ) -> Result<Option<MessageRow>, RunError> {
        Ok(self
            .db
            .move_message(agent, session, message, from, to)?
            .map(|(moved, event)| {
                self.bus.publish(Notice::new(
                    event,
                    SESSION_MESSAGE_STATE,
                    &message_notice(agent, session, &moved),
                ));
                moved
            }))
    }

    /// Types the oldest message waiting for the session, if its agent is
    /// idle, and holds the others with why; sends them to the Queue when
    /// Pinrail no longer runs the session.
    pub async fn deliver(&self, agent: &str, session: &str) -> Result<(), RunError> {
        let waiting = self.db.messages_to_type(agent, session)?;
        let Some(first) = waiting.first() else {
            return Ok(());
        };
        let Some(run) = self.typing_run(agent, session)? else {
            for message in &waiting {
                self.to_queue(agent, session, message.id)?;
            }
            return Ok(());
        };
        let presence = self.db.presence(agent, session)?;
        let hold = match presence.as_ref().map(|p| p.state.as_str()) {
            Some("working") => Some("working"),
            Some("needs_input") => Some("needs_input"),
            Some("stopped") => {
                for message in &waiting {
                    self.to_queue(agent, session, message.id)?;
                }
                return Ok(());
            }
            _ => None,
        };
        if let Some(why) = hold {
            for message in waiting.iter().filter(|m| m.held.as_deref() != Some(why)) {
                self.move_message(
                    agent,
                    session,
                    message.id,
                    &["queued"],
                    MessageChange {
                        state: "queued",
                        held: Some(why),
                        ..MessageChange::default()
                    },
                )?;
            }
            return Ok(());
        }
        // idle, or no hooks to say otherwise
        let idle_since = presence
            .filter(|p| p.state == "idle")
            .map(|p| p.state_since);
        self.type_one(
            agent,
            session,
            first.id,
            &run,
            idle_since,
            false,
            &["queued"],
        )
        .await
    }

    /// Types a message the person retries, or sends anyway although they
    /// used the terminal: one that is unconfirmed, or still waits.
    pub async fn type_now(
        &self,
        agent: &str,
        session: &str,
        message: i64,
        anyway: bool,
    ) -> Result<(), RunError> {
        let run = self
            .typing_run(agent, session)?
            .ok_or(RunError::NotTyped("exited"))?;
        let idle_since = self
            .db
            .presence(agent, session)?
            .filter(|p| p.state == "idle")
            .map(|p| p.state_since);
        self.type_one(
            agent,
            session,
            message,
            &run,
            idle_since,
            anyway,
            &["queued", "unconfirmed"],
        )
        .await
    }

    /// Types one message into the run's terminal, and moves it on as the
    /// host answers: `typing` until its prompt confirms it, `typed` for an
    /// agent without a prompt hook, or back to waiting with why.
    #[allow(clippy::too_many_arguments)]
    async fn type_one(
        &self,
        agent: &str,
        session: &str,
        message: i64,
        run: &RunRow,
        idle_since: Option<DateTime<Utc>>,
        anyway: bool,
        from: &[&str],
    ) -> Result<(), RunError> {
        let Some(typing) = self.move_message(
            agent,
            session,
            message,
            from,
            MessageChange {
                state: "typing",
                route: Some("type"),
                run: Some(&run.id),
                ..MessageChange::default()
            },
        )?
        else {
            // a status claimed it, or another call types it
            return Ok(());
        };
        let confirmed_by_hook = pinrail_runtime::adapter(agent)
            .and_then(|a| a.submit())
            .is_some_and(|plan| plan.confirms.is_some());
        fn back(held: Option<&str>) -> MessageChange<'_> {
            MessageChange {
                state: "queued",
                held,
                ..MessageChange::default()
            }
        }
        match self
            .type_message(&run.id, &typing.text, idle_since, anyway)
            .await
        {
            Ok(()) if confirmed_by_hook => {}
            Ok(()) => {
                self.move_message(
                    agent,
                    session,
                    message,
                    &["typing"],
                    MessageChange {
                        state: "typed",
                        ..MessageChange::default()
                    },
                )?;
            }
            Err(RunError::NotTyped(why @ ("used_since_idle" | "not_foreground"))) => {
                self.move_message(agent, session, message, &["typing"], back(Some(why)))?;
            }
            Err(RunError::NotTyped("exited")) => {
                self.move_message(
                    agent,
                    session,
                    message,
                    &["typing"],
                    MessageChange {
                        state: "queued",
                        route: Some("queue"),
                        ..MessageChange::default()
                    },
                )?;
            }
            Err(error) => {
                self.move_message(agent, session, message, &["typing"], back(None))?;
                return Err(error);
            }
        }
        Ok(())
    }

    /// Sends a message to the Queue, for the agent's next status.
    fn to_queue(&self, agent: &str, session: &str, message: i64) -> Result<(), RunError> {
        self.move_message(
            agent,
            session,
            message,
            &["queued"],
            MessageChange {
                state: "queued",
                route: Some("queue"),
                ..MessageChange::default()
            },
        )?;
        Ok(())
    }

    /// Delivers the typed message of the session that the prompt its hook
    /// reported confirms; see [`confirms`].
    pub fn confirm(
        &self,
        agent: &str,
        session: &str,
        prompt: &str,
        cut: bool,
    ) -> Result<(), RunError> {
        let typed = self.db.typed_messages(agent, session)?;
        if let Some(message) = typed.iter().find(|m| confirms(prompt, &m.text, cut)) {
            self.move_message(
                agent,
                session,
                message.id,
                &["typing", "unconfirmed"],
                MessageChange {
                    state: "delivered",
                    ..MessageChange::default()
                },
            )?;
        }
        Ok(())
    }

    /// Shows a typed message that no prompt confirmed within 10 seconds as
    /// unconfirmed, and sends one that waited 30 minutes to the Queue.
    pub fn sweep_messages(&self) -> Result<(), RunError> {
        let now = Utc::now();
        let (unconfirmed, overdue) = self
            .db
            .overdue_messages(now - CONFIRM_WITHIN, now - HOLD_FOR)?;
        for (agent, session, message) in unconfirmed {
            self.move_message(
                &agent,
                &session,
                message,
                &["typing"],
                MessageChange {
                    state: "unconfirmed",
                    ..MessageChange::default()
                },
            )?;
        }
        for (agent, session, message) in overdue {
            self.to_queue(&agent, &session, message)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::confirms;

    #[test]
    fn a_prompt_confirms_the_same_text_or_a_longer_one_that_starts_with_it() {
        assert!(confirms("Use v2.", "Use v2.", false));
        assert!(confirms("  Use v2.\r\nand hurry ", "Use v2.", false));
        assert!(confirms(
            "line one\nline two",
            "line one\r\nline two",
            false
        ));
        assert!(!confirms("Use v3.", "Use v2.", false));
        assert!(!confirms("Use", "Use v2.", false));
        assert!(!confirms("anything", "   ", false));
    }

    #[test]
    fn a_prompt_cut_to_its_excerpt_confirms_a_message_that_starts_with_it() {
        assert!(confirms("Use", "Use v2.", true));
        assert!(!confirms("Use v3", "Use v2.", true));
    }
}
