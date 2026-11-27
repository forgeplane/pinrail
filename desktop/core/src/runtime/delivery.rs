//! Messages to an agent Pinrail runs, typed into its terminal.
//! A message to a session with a live run whose agent has a typing plan
//! goes by the Type route; any other goes by the Queue, to the agent's
//! next status. A message to type waits while the agent works or waits for
//! an answer, and is typed once it is idle, one at a time; the agent's
//! prompt hook confirms it, whatever the person did in the terminal, and
//! without hooks, the agent's terminal reporting work right after it. A
//! message waits, too, until the agent first reports a state, so that it
//! is not typed while the agent starts; the person can type it sooner. A
//! message is typed once by itself: after that, the person retries it. One held for 30 minutes, or whose run
//! ended, goes to the Queue.

use chrono::Utc;

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
/// confirms a message that starts with it. The paths of files attached to
/// the message, and the `[Image #N]` an agent shows in place of an image
/// it attached, are left out of both: Claude Code takes an image's path
/// out of the prompt and puts the placeholder first.
pub fn confirms(prompt: &str, typed: &str, cut: bool) -> bool {
    let lines = |s: &str| s.replace("\r\n", "\n").replace('\r', "\n");
    if lines(typed).trim().is_empty() {
        return false;
    }
    let prompt = without_attachments(&lines(prompt)).trim().to_string();
    let typed = without_attachments(&lines(typed)).trim().to_string();
    if typed.is_empty() {
        // a message of files alone, which its prompt is too
        return prompt.is_empty();
    }
    prompt.starts_with(&typed) || (cut && typed.starts_with(&prompt))
}

/// A text without the lines that are paths of files attached to a session
/// (`/…/sessions/<agent>/<session>/files/<name>`, quoted or not), and
/// without the `[Image #N]` placeholders an agent shows for an image.
fn without_attachments(text: &str) -> String {
    let kept: Vec<&str> = text.lines().filter(|l| !attached_path(l)).collect();
    let mut out = String::new();
    let mut rest = kept.join("\n");
    while let Some(at) = rest.find("[Image #") {
        let after = &rest[at + "[Image #".len()..];
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && after[digits..].starts_with(']') {
            out.push_str(&rest[..at]);
            rest = after[digits + 1..].to_string();
        } else {
            out.push_str(&rest[..at + 1]);
            rest = rest[at + 1..].to_string();
        }
    }
    out.push_str(&rest);
    out
}

/// Whether a line is only the path of a file attached to a session.
fn attached_path(line: &str) -> bool {
    let line = line.trim();
    let path = line
        .strip_prefix('\'')
        .and_then(|l| l.strip_suffix('\''))
        .unwrap_or(line);
    // on Windows, `C:\…\sessions\…` with either separator
    let path = path.replace('\\', "/");
    let Some((_, tail)) = path.rsplit_once("/sessions/") else {
        return false;
    };
    let parts: Vec<&str> = tail.split('/').collect();
    let bytes = path.as_bytes();
    let absolute = path.starts_with('/')
        || bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'/';
    absolute && parts.len() == 4 && parts[2] == "files" && parts.iter().all(|p| !p.is_empty())
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
        // with no run to type into, those sent to be typed go to the queue;
        // those there already stay as they are
        let to_queue = |runtime: &Self| -> Result<(), RunError> {
            for message in waiting
                .iter()
                .filter(|m| m.route.as_deref() == Some("type"))
            {
                runtime.to_queue(agent, session, message.id)?;
            }
            Ok(())
        };
        let Some(run) = self.typing_run(agent, session)? else {
            return to_queue(self);
        };
        let presence = self.db.presence(agent, session)?;
        let hold = match presence.as_ref().map(|p| p.state.as_str()) {
            // nothing has said the agent is ready for a prompt yet
            None => Some("starting"),
            Some("working") => Some("working"),
            Some("needs_input") => Some("needs_input"),
            Some("stopped") => return to_queue(self),
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
        // idle
        self.type_one(agent, session, first.id, &run, &["queued"])
            .await
    }

    /// Types a message the person retries: one that is unconfirmed, or
    /// still waits.
    pub async fn type_now(&self, agent: &str, session: &str, message: i64) -> Result<(), RunError> {
        let run = self
            .typing_run(agent, session)?
            .ok_or(RunError::NotTyped("exited"))?;
        self.type_one(agent, session, message, &run, &["queued", "unconfirmed"])
            .await
    }

    /// Types one message into the run's terminal, and moves it on as the
    /// host answers: `typing` until its prompt confirms it, `typed` for an
    /// agent without a prompt hook, or back to waiting with why.
    /// The text to type for `message`: after Pinrail's note when it is the
    /// first message typed into a run that started its agent without one,
    /// as a start with no first task does. The message keeps the person's
    /// words, which the agent's prompt, without the note, confirms.
    fn with_note(&self, run: &RunRow, message: &MessageRow) -> Result<String, RunError> {
        let noted = |argv: &[String]| argv.iter().any(|a| a.contains("<pinrail>"));
        let needs = run.kind == "start"
            && !self.db.run_argv(&run.id)?.is_some_and(|argv| noted(&argv))
            && !self.db.typed_in_run(&run.id, message.id)?;
        Ok(if needs {
            crate::preamble::before(&crate::preamble::for_session(), &message.text)
        } else {
            message.text.clone()
        })
    }

    async fn type_one(
        &self,
        agent: &str,
        session: &str,
        message: i64,
        run: &RunRow,
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
            // typed whenever the agent is idle, whatever the person did in
            // the terminal, which the host cannot read
            .type_message(&run.id, &self.with_note(run, &typing)?, None, false)
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
            Err(RunError::NotTyped(why @ "not_foreground")) => {
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

    /// Delivers the message being typed into the session's agent, now that
    /// its terminal reports work: Pinrail types only into an idle agent, so
    /// the work is the message's. Hooks off, nothing else confirms it.
    pub fn confirm_by_work(&self, agent: &str, session: &str) -> Result<(), RunError> {
        let typed = self.db.typed_messages(agent, session)?;
        if let Some(message) = typed.iter().find(|m| m.state == "typing") {
            self.move_message(
                agent,
                session,
                message.id,
                &["typing"],
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
    use super::{attached_path, confirms};

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
    fn the_path_of_a_file_attached_on_windows_is_recognised() {
        for line in [
            r"C:\Users\p\AppData\Local\Pinrail\sessions\claude-code\ses_1\files\image.png",
            r"'C:\Users\p\AppData\Local\Pinrail\sessions\claude-code\ses_1\files\my notes.md'",
            "C:/Users/p/AppData/Local/Pinrail/sessions/claude-code/ses_1/files/image.png",
        ] {
            assert!(attached_path(line), "{line}");
        }
        assert!(!attached_path(
            r"sessions\claude-code\ses_1\files\image.png"
        ));
    }

    #[test]
    fn a_prompt_the_agent_rewrote_around_attached_files_confirms_the_message() {
        let image = "/data/sessions/claude-code/ses_1/files/image.png";
        let notes = "'/data/sessions/claude-code/ses_1/files/my notes.md'";
        // Claude Code takes an image's path out and shows the image first
        assert!(confirms(
            "[Image #1]what do you see here?",
            &format!("what do you see here?\n\n{image}"),
            false
        ));
        // a file it does not attach keeps its path
        assert!(confirms(
            &format!("Read these [Image #1]\n\n{notes}"),
            &format!("Read these\n\n{image}\n{notes}"),
            false
        ));
        // files alone
        assert!(confirms("[Image #1]", image, false));
        assert!(!confirms(
            "Something else",
            &format!("Look\n\n{image}"),
            false
        ));
    }

    #[test]
    fn a_prompt_cut_to_its_excerpt_confirms_a_message_that_starts_with_it() {
        assert!(confirms("Use", "Use v2.", true));
        assert!(!confirms("Use v3", "Use v2.", true));
    }
}
