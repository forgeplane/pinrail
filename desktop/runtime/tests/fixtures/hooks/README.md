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
