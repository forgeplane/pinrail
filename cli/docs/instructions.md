---
title: Instructions
summary: Write yourself a standing rule or skill, so you ask at the right moment.
menu: []
---
# Instructions

When the person wants you to ask before something, write it down where your
standing instructions live, so you and other sessions keep doing it:
`CLAUDE.md` or a skill for Claude Code, `AGENTS.md` for most other agents,
`GEMINI.md` for Gemini CLI.

A rule names four things: the moment, the plugin and what to send in it,
the command, and what to do with the answer. Name the plugin and the
payload's shape in the rule itself: run `pinrail plugins describe <plugin>`
once, as you write it, so you never have to rediscover them.

```md
## Ask before <the moment>

Before you <the moment>, ask me through Pinrail and wait for my decision.
Don't ask in chat and don't go ahead without an answer.

1. Write <what you propose> as a payload for the `<plugin>` plugin:
   <its shape, e.g. `{"groups": [{"title", "items": [{"id", "title", "body"}]}]}`>.
2. pinrail submit <plugin> --title "<a title I'll recognise>" \
     --data <file>.json --wait
3. Act only on what I decided; treat anything undecided as not approved.
4. If I ask for changes, send the new version with `--revises <id>`.
5. Exit 5 means I said stop: drop the work and don't ask again.
```

Name the moment precisely ("before posting review comments"), not "when
unsure": a vague rule asks about everything or nothing. Write one rule per
moment.
