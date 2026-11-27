//! The note Pinrail puts before the first task of an agent it starts, in a
//! `<pinrail>` block, so that the agent knows it works in Pinrail and how
//! to read and claim work items. The block is removed again from the
//! prompt the agent reports, so the thread shows the person's own words.

const OPEN: &str = "<pinrail>";
const CLOSE: &str = "</pinrail>";

/// The note for a session the person starts with a first message.
pub fn for_session() -> String {
    format!(
        "{OPEN}\n\
         Pinrail started this session. Pinrail is the app where the person \
         answers your questions and reviews your work. Use the pinrail skill \
         to ask the person questions and to submit work for review.\n\
         A key such as WEB-42 names a Pinrail work item. Read the item with \
         `pinrail items show WEB-42`, and claim it with \
         `pinrail items edit WEB-42 --claim` before you start working on it.\n\
         Start each task with `pinrail status --start \"<what you are doing>\"`.\n\
         {CLOSE}"
    )
}

/// The note for a session the person starts on the work item `key`, whose
/// text follows it.
pub fn for_item(key: &str) -> String {
    format!(
        "{OPEN}\n\
         Pinrail started this session for the work item {key}, which follows \
         this note. Pinrail is the app where the person answers your questions \
         and reviews your work. Use the pinrail skill to ask the person \
         questions and to submit work for review.\n\
         Start with `pinrail status --start \"<what you are doing>\"`. Comment \
         on the item with `pinrail items comment {key} \"…\"` as you learn \
         things, and mark it done with `pinrail items done {key} --comment \"…\"` \
         once the person has approved the work.\n\
         {CLOSE}"
    )
}

/// `task` with `note` before it.
pub fn before(note: &str, task: &str) -> String {
    format!("{note}\n\n{task}")
}

/// A prompt without the note at its start, if it has one. Only a block
/// that opens the prompt, with its tags on lines of their own, is the
/// note: one quoted later in the prompt is the person's text.
pub fn without(prompt: &str) -> String {
    let start = prompt.trim_start();
    let Some(rest) = start.strip_prefix(OPEN) else {
        return prompt.to_string();
    };
    let Some(body) = rest
        .strip_prefix('\n')
        .or_else(|| rest.strip_prefix("\r\n"))
    else {
        return prompt.to_string();
    };
    let mut at = 0;
    for line in body.split_inclusive('\n') {
        if line.trim_end() == CLOSE {
            return body[at + line.len()..].trim_start().to_string();
        }
        at += line.len();
    }
    prompt.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_after_the_note_is_the_persons_text() {
        let prompt = before(&for_session(), "Fix the login\nand add a test");
        assert_eq!(without(&prompt), "Fix the login\nand add a test");
        let prompt = before(&for_item("WEB-42"), "# WEB-42 Fix the login");
        assert_eq!(without(&prompt), "# WEB-42 Fix the login");
    }

    #[test]
    fn a_prompt_without_the_note_is_kept() {
        assert_eq!(without("Fix the login"), "Fix the login");
    }

    #[test]
    fn a_note_quoted_after_the_start_is_kept() {
        let prompt = "Why does this show?\n<pinrail>\nnote\n</pinrail>";
        assert_eq!(without(prompt), prompt);
    }

    #[test]
    fn a_note_that_is_never_closed_is_kept() {
        let prompt = "<pinrail>\nnote with no end";
        assert_eq!(without(prompt), prompt);
    }
}
