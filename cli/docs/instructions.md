---
title: Instructions
summary: Write yourself a standing rule or skill, so you ask at the right moment.
menu: []
---
# Instructions

When the person wants you to ask before something, write it down where your
standing instructions live, so you and other sessions keep doing it:
`CLAUDE.md` or a skill for Claude Code, `GEMINI.md` for Gemini CLI,
`AGENTS.md`, or a skill where your agent supports them, for the others.

A rule names four things: the moment, the plugin and what to send in it,
the command, and what to do with the answer.

Pick the plugin first. One fits when it both shows what the person needs
to see and returns the decision you need:

- `pinrail plugins` lists the installed plugins and says when to use each
  one. Choose among them. `pinrail plugins describe <plugin>
  --decision-schema` shows what a plugin returns. Use it to check that the
  plugin returns the decision you need.
- Showing the right material is not enough: a diff view does not make a
  plugin fit for approving a commit if it does not return an approval.
- Don't bend the task to fit a schema. Don't make up an item or a comment
  just to have something to decide on.
- If no plugin represents the decision, build one (see
  `pinrail docs plugins/building`), then write the rule for it. That is a
  normal step, not a last resort.

Name the plugin and the payload's shape in the rule itself. Run
`pinrail plugins describe <plugin>` once while you write the rule, so you
never have to look them up again.

```md
## Ask before <the moment>

Before you <the moment>, ask me through Pinrail and wait for my decision.
Don't ask in chat and don't go ahead without an answer.

1. Write <what you propose> as a payload for the `<plugin>` plugin:
   <its shape, as `pinrail plugins describe <plugin>` gives it>.
2. pinrail submit <plugin> --title "<a title I'll recognise>" \
     --data <file>.json --wait
3. Act only on what I decided; treat anything undecided as not approved.
4. If I ask for changes, send the new version with `--revises <id>`.
5. Exit 5 means I said stop: drop the work and don't ask again.
```

Name the moment precisely ("before posting review comments"), not "when
unsure": a vague rule asks about everything or nothing. Write one rule per
moment.
