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

## Development

Tool versions are pinned in `mise.toml` (Erlang, Elixir, Rust); `mise install`
sets them up.

### Server

```sh
cd server
mix setup          # deps, Tailwind and esbuild
mix phx.server     # http://127.0.0.1:4747
mix test           # unit and LiveView tests
```

Data lives under `$XDG_DATA_HOME/wicket` (`~/.local/share/wicket`), or
`WICKET_DATA_DIR` if set. `WICKET_PORT` overrides the port.

### Browser tests

The gate page talks to a sandboxed iframe over `postMessage`, which only a
real browser can exercise. Those tests drive headless Chromium through
Playwright and run only when asked for.

One-time setup, from `server/`:

```sh
npm --prefix assets install
npx --prefix assets playwright install chromium
```

Then:

```sh
mix test.browser                  # only the browser tests
mix test --include playwright     # everything
```

Tests live in `test/wicket_web/browser/` and are tagged `:playwright`.
