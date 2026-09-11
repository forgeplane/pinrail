# wicket server

The Phoenix application: the HTTP API workflows talk to, the inbox and gate
pages a human decides in, and the plugin registry. Elixir 1.20, Phoenix 1.8,
LiveView, Tailwind. Storage is plain files; there is no database.

## Run

```sh
mix setup          # deps, Tailwind and esbuild
mix phx.server     # http://127.0.0.1:4747
```

| Variable | Meaning | Default |
|---|---|---|
| `WICKET_DATA_DIR` | gates, decisions, plugin snapshots, `server.json` | `$XDG_DATA_HOME/wicket`, i.e. `~/.local/share/wicket` |
| `WICKET_CONFIG_DIR` | registered plugin directories | `$XDG_CONFIG_HOME/wicket`, i.e. `~/.config/wicket` |
| `WICKET_PORT` | listen port, loopback only | `4747` |
| `USER` | `decided_by` on decisions | `wicket` |

A release generates its cookie secret into the data dir on first boot and
needs no other environment.

## Layout on disk

```
<data dir>/
  gates/<id>/gate.json        envelope + payload, written once
  gates/<id>/decision.json    the decision, created exclusively, never rewritten
  gates/<id>/withdrawn.json   withdrawal timestamp
  gates/<id>/events.jsonl     created, viewed, decided, withdrawn, expired
  plugins/<name>/<version>/   snapshot of a plugin the first time a gate used it
  server.json                 url, port and pid while the server runs
```

Status is derived from those files and `expires_at`; nothing is edited in
place. An in-memory index of envelopes serves listing and filtering and is
rebuilt from the files at boot.

## Code map

| Module | Role |
|---|---|
| `Wicket.Gates` | create, get, list, decide, withdraw, expiry sweep, PubSub |
| `Wicket.Store` | the only writer to the data dir; atomic and exclusive writes |
| `Wicket.Gates.Index` | ETS index of envelopes |
| `Wicket.Types`, `Wicket.Types.Registry`, `Wicket.Types.Plugin` | plugin discovery, JSON Schema validation, snapshots |
| `Wicket.GateError` | the one error struct: reason, JSON-pointer violations, HTTP status |
| `WicketWeb.API.*` | `/api/gates`, `/api/types` |
| `WicketWeb.PluginController` | serves plugin bundles with the sandbox CSP |
| `WicketWeb.InboxLive`, `GateLive`, `HistoryLive`, `TypesLive` | the pages |
| `assets/js/hooks/plugin_bridge.js` | the shell side of the plugin protocol |
| `priv/plugins/list` | the built-in gate type |
| `priv/static/sdk/v1/wicket-plugin.js` | the plugin SDK, copied from `wicket_sdk/` by `mix assets.build` |

## Tests

```sh
mix test               # unit and LiveView tests
mix test.browser       # Playwright browser tests, see below
```

Browser tests drive headless Chromium and run only on request. One-time
setup:

```sh
npm --prefix assets install
npx --prefix assets playwright install chromium
```

They live in `test/wicket_web/browser/`, tagged `:playwright`. Test fixture
plugins are in `test/fixtures/plugins/`.

The shell's own JavaScript, including the plugin bridge, has unit tests that
need no browser:

```sh
npm --prefix assets test      # or mise run test:shell
```

## App controls

The sidebar collapses with ⌘/Ctrl+B and the theme switches with T; both are
remembered per browser in `localStorage` and neither reloads the page, so an
open gate view keeps its state. `J` and `K` move between inbox rows, `Enter`
opens the focused one and `/` focuses search. Inbox searches and history
filters live in the URL, so a filtered view can be linked or reloaded.

## The plugin bridge

`assets/js/hooks/plugin_bridge.js` is the shell half of the plugin protocol.
Beyond relaying messages it guards the decision: the agent note is read from
the DOM at submit time rather than from the last debounced assign, a second
submit is ignored while one is in flight, and a submit made while the socket
is down comes back to the view as a violation instead of being lost. Drafts in
`sessionStorage` are a convenience only; the decision files on the server
remain authoritative.
