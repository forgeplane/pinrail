//! The skill's text for the person's settings. Where each agent keeps its
//! skills is in the registry.

/// The skill's description, in place of `{description}` in its front
/// matter.
const DESCRIPTION: &str = "Ask the person questions or submit a review to them through Pinrail and wait for their answer, or build a Pinrail plugin. Use when you need an answer only the person can give, or when a prompt, a skill or your instructions tell you to ask, check with, or get approval from the person through Pinrail.";

/// The skill's text, with `{description}` replaced by its description.
pub fn skill_text(text: &str) -> String {
    text.replace("{description}", DESCRIPTION)
}

#[cfg(test)]
mod tests {
    use super::skill_text;

    #[test]
    fn the_skill_gets_its_description() {
        let text = skill_text("description: {description}\n");
        assert!(
            text.starts_with("description: Ask the person questions"),
            "{text}"
        );
    }
}
