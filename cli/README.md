# wicket CLI

The command agents call from a shell. It talks HTTP to the running server
and does nothing itself: JSON on stdout, diagnostics on stderr, exit codes
that a workflow can branch on.

```sh
cargo build --release      # target/release/wicket
cargo test                 # integration tests against a scripted HTTP server
```

## Finding and starting the server

In order: `--url` or `WICKET_URL`; the `server.json` the running server
writes into the data dir (`WICKET_DATA_DIR`, else `~/.local/share/wicket`);
`http://127.0.0.1:4747`.

`wicket serve` starts the server when nothing answers, and `wicket create`
does the same unless `--no-start` is given. Starting needs one of:

| Variable | Meaning |
|---|---|
| `WICKET_SERVER_CMD` | a shell command that runs the server |
| `WICKET_SERVER_DIR` | the Phoenix app directory; runs `mix phx.server` there |

The server runs detached in its own process group, logging to
`<data dir>/server.log`.

## Commands

```
wicket create <type> --title T [--source repo=…,workflow=…,run_id=…,ref=…,url=…]
              [--data FILE|-] [--summary JSON] [--supersedes ID] [--expires-at ISO]
              [--requested-by NAME] [--wait] [--timeout SECS] [--decision-out FILE]
wicket wait <id> [--timeout SECS] [--decision-out FILE]
wicket show <id>
wicket list [--status S[,S]] [--repo R] [--workflow W] [--ref X] [--run-id R] [--type T]
            [--limit N] [--superseded]
wicket decide <id> --data FILE|- [--note TEXT] [--by NAME]
wicket withdraw <id> [--reason TEXT]
wicket discard <id> [--reason TEXT] [--by NAME]
wicket types | wicket types add <dir> | wicket types reload
wicket serve
wicket open <id>
```

`--pretty` on any command pretty-prints the output.

`create --wait` is the one-liner for workflows: create the gate, print its
URL to stderr, block until a human decides, print the envelope, exit 0. With
`--decision-out`, `decision.data` is written to that file as indented JSON,
so a workflow that already reads a decisions file keeps working unchanged.

`wait` polls the server in short long-polls and retries on 204 or a dropped
connection, so it survives the server restarting. A poll that lands on a
server in the middle of shutting down is abandoned after about 20 seconds.

## Exit codes

| code | meaning |
|---|---|
| 0 | done; for `wait`, the gate was decided |
| 1 | error: bad arguments, server unreachable, I/O |
| 2 | the server refused the request (404, 409, 422); its JSON body is on stderr |
| 3 | the gate was withdrawn or expired instead of decided |
| 4 | `wait` timed out; the gate is still pending |
| 5 | the person discarded the gate: stop the work it was gating |

A discard is the person's "no, and stop", made in the app or with `wicket
discard`. The envelope carries `discarded_by` and `discarded_reason`; an
agent that gets exit 5 stops the work the gate was about, reports the
reason, and neither retries nor posts anything. `--decision-out` writes
nothing: there is no decision.
