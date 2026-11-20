# Hook input fixtures

The JSON each agent passes a hook on stdin, as `tests/hooks.rs` reads it.
Paths are rewritten to `/Users/you/code/app`.

**Claude Code 2.1.322, captured** from `claude -p` with hooks that saved
their input: `session-start`, `prompt`, `handback-prompt` (the prompt that
hands a subagent's result back), `tool-done`, `subagent-tool-done`,
`permission`, `stop` and `session-end`.

**Claude Code 2.1.323, recorded by Pinrail** from an interactive session:
`task-notification-prompt`, the prompt that says a background command
finished, with its paths replaced.

**Claude Code, built from the documented fields**, for events a headless run
does not fire: `permission-denied`, `notification-*`, `stop-failure`,
`cursor-fired` (Cursor running Claude Code's hooks), `grok-fired` (Grok,
with its camelCase `hookEventName`); the tests run these without Claude
Code's `CLAUDE_CODE_SESSION_ID`, as Cursor and Grok do and `session-start-unknown-source`.

**Codex, built from its schemas** in `codex-rs/hooks/schema/generated/`:
every file in `codex/`.

**Claude Code's status file**, `tests/fixtures/status/claude-code-idle.json`:
`~/.claude/sessions/<pid>.json` of Claude Code 2.1.323, captured seconds after
an interrupt, with its paths rewritten.

**Antigravity CLI 1.5.12, captured** from an interactive session with a
workspace `.agents/hooks.json` whose hooks saved their input:
`session-start`, `first-invocation` (the first `PreInvocation` of a turn),
`next-invocation`, `tool-done` (`PostToolUse`) and `stop`. Built from the
same fields: `stop-canceled` and `stop-error`. `transcript_full.jsonl` has
the shape of the transcript agy keeps, with made-up content.

**Grok CLI 1.0.89, captured** from interactive sessions with a project
`.grok/hooks/pinrail.json` whose hooks saved their input, with the paths and
the session's id rewritten: `session-start`, `prompt`, `pre-tool-use`
(`PreToolUse` of `run_terminal_command`), `notification-permission`,
`permission-denied`, `stop-cancelled-rejected` (a declined permission),
`pre-tool-use-question` (`ask_user_question`), `notification-question`,
`stop-cancelled-interrupt` (Ctrl+C), `stop-shutdown` and `session-end`.
Built from the same fields: `stop`, a turn that ended with its reply.

**Copilot CLI 1.0.114, captured** from interactive sessions with
`COPILOT_HOME` in a scratch folder and hooks that saved their input, with
the paths and the session's id rewritten: `session-start`,
`session-start-resume`, `prompt`, `pre-tool-use` (`bash`),
`pre-tool-use-ask-user`, `post-tool-use`, `permission-request`,
`notification-permission`, `notification-question`,
`notification-shell-completed`, `agent-stop` and `session-end`.
`events.jsonl` has the shape of the transcript Copilot keeps, with made-up
content; `events-pending.jsonl` ends with a prompt whose reply is not
written yet.

**Pi 1.5.2, from Pinrail's extension** (`src/agents/pi-extension.ts`), whose
reports were captured from interactive sessions for `session-start`,
`session-start-new`, `prompt`, `session-end` and `session-end-new`, with the
paths rewritten. The others are built from the same fields, for the events
Pi's extension API documents and the research saw: `prompt-steer`, `tool`,
`waiting-confirm`, `waiting-select`, `answered`, `stop`, `stop-aborted` and
`stop-error`.

**Cursor's CLI 2026.11.19, captured** from interactive sessions with a
project `.cursor/hooks.json` whose hooks saved their input, with the paths,
the conversation's id and the email rewritten: `session-start`, `prompt`
(`beforeSubmitPrompt`), `pre-tool-use`, `post-tool-use`,
`after-agent-response`, `stop` (completed), `stop-aborted` and `stop-error`
(Ctrl+C sends both) and `session-end`.
