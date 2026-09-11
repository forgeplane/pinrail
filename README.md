# wicket

A wicket is the small gate in a larger door: a person waits at it, looks at
what is being carried through, and lets it pass or not.

wicket is a standalone approval-gate service for agent workflows. A workflow
that is about to do something outward-facing (post review comments, silence a
Sentry issue, open an issue, push a branch) creates a gate with a JSON payload
and blocks. The web app shows the gate in an inbox, renders it with the view
registered for its type, records the human's decision, and the blocked
workflow resumes with that decision. Every decision is kept, so past rounds
can be rendered again and rejection reasons become training data.

Stack: Elixir, Phoenix LiveView and plain-file storage for the server
(`server/`). The CLI agents call is a Rust binary (`cli/`) talking HTTP to the
running app.

Status: early development.

## Layout

| Directory | Contents |
|---|---|
| [`server/`](server/README.md) | the Phoenix app: API, web UI, plugin registry |
| [`cli/`](cli/README.md) | the Rust CLI workflows call |
| [`plugins/`](plugins/README.md) | the plugin protocol, the official `review` plugin and a sample |
| [`e2e/`](e2e/README.md) | end-to-end tests: server, CLI and plugins together |

Tool versions are pinned in `mise.toml`; `mise install` sets them up.
`mise run test` runs every suite; `mise run e2e`, `mise run test:server` and
`mise run test:cli` run one.

## Quick start

```sh
cd server && mix setup && mix phx.server        # http://127.0.0.1:4747
cd cli && cargo build --release                 # target/release/wicket

wicket create list --title "MR !42" --source repo=acme,workflow=review,ref=42 \
  --data payload.json --wait --decision-out mr-42.decisions.json
```

The command blocks until someone decides the gate in the browser, then
prints the decision and exits 0 (3 if the gate was withdrawn, 4 on timeout).
Payload and decision shapes for the built-in type are in
[`server/priv/plugins/list`](server/priv/plugins/list/README.md).
