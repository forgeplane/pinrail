# Terminal benchmark

This benchmark measures the window's terminal renderers against the window
budgets of runtime-spec 8.6. It loads each renderer from the app's registry
(`ui/src/components/terminal/renderers/`), so it measures the code the app
ships, through the `TerminalView` interface the app uses.

## Running it

```sh
mise run bench:terminal                                  # every case, both renderers
mise run bench:terminal -- --repeat 3                    # best of three, as spec 8.6 asks
mise run bench:terminal -- --adapters ghostty --cases burst,input
```

The task builds libghostty-vt's WebAssembly first if needed. The first run
also writes the synthetic corpus and asks the host's terminal for the attach
snapshots, which needs Cargo. Each run prints a table and writes
`results/<time>.json` and `results/<time>.md`, which it also copies to
`results/latest.json` and `results/latest.md`. The exit status is 1 when a
budget fails. Pass `--no-fail` to always exit with 0.

The options of `run.mjs`:

- `--runners`: where the cases run.
  - `webkit`, the default: Playwright's headless WebKit.
  - `wkwebview`: macOS's system WKWebView in a real window, as the app
    draws. The window appears at the bottom left of the screen without
    taking focus, and keeps drawing when covered.
  - `webkitgtk`: WebKitGTK 4.1, the library Tauri uses on Linux.
    `runners/webkitgtk/run-in-docker.sh` runs it in the Docker image of
    `mise run linux-desktop`, with a virtual display and software drawing,
    so its numbers show that the path works rather than meet the budgets.
  - `app`: the app's own window, under the app's development policy. It
    needs `mise run dev:desktop` running: a window opens in that app, in
    front and with the keyboard, runs every case, and closes. While it is
    covered, macOS slows its animation frames, so leave it in front. Memory
    is not measured, as the window shares its process with the app's.
    Release builds have neither the window nor its page (`app.html`).
  - `external`: prints the URL of each case, to open in another browser.
- `--adapters`: the renderers by id, `ghostty,xterm` by default. It also
  takes `ghostty-web`, coder/ghostty-web, which the app does not register.
  The benchmark measures it beside the libghostty renderer for comparison,
  through `adapters/ghostty-web.ts`, whose header lists what it leaves out.
- `--cases`: the cases by name, all of them by default.
- `--repeat N`: runs each case N times in a new browser and keeps the best.
- `--tag NAME`: adds NAME to the result file names.
- `--no-build`: uses the existing bundle in `dist/`.
- `--baseline FILE`: also fails when a measure is more than 15 % worse than
  the baseline's and worse by more than its margin, such as 5 ms for the
  input latency or 17 ms, about a frame, for the longest frame.
  `baseline.mjs` lists the measures and their margins.
- `--write-baseline FILE`: writes this run's measures as a baseline.
- `--skip-budgets`: checks the baseline only, for machines other than the
  reference ones.

## In CI

`.github/workflows/bench-terminal.yml` runs every case except the replays
and the screenshots, best of three, in WebKit, on each change to the
renderers, the benchmark, the screen or the host. CI's machines are not the
reference machines, so it compares with the baseline of the last push to
main on the same runner type rather than checking the budgets. A passing
run on main becomes the next baseline, and each run's results are kept as
an artifact.

To open a case by hand, start the fake core with `node core/server.mjs` and
open `http://127.0.0.1:7357/page.html?adapter=ghostty&case=burst&item=synthetic/tui.bin`.
The page prints the result in the console. Add `&format=vt` to make the
attach, memory and switch cases ask for the `vt` snapshot, and `&csp=off` to
load the page without the app's Content-Security-Policy.

## How it works

- `page.ts` is the bench page. It loads one renderer, runs one case and
  posts the result back to the fake core. `build.mjs` bundles it into
  `dist/`, with a chunk for each renderer.
- `core/server.mjs` is the fake core. It serves the page with the app's
  Content-Security-Policy, read from `src-tauri/tauri.conf.json`, so a
  renderer that needs more than the app allows reports violations. It plays
  corpus items to the page over a WebSocket with credit-based flow control,
  as the core does: a 1 MiB window, with credit returned once the view has
  parsed the bytes. It also runs the echo program of the input case
  (`core/pty-echo.py`).
- `core/snapshots.mjs` asks the host's terminal
  (`desktop/screen/examples/bench_snapshot.rs`) for a terminal of 120
  columns and 40 rows with 5,000 lines of history, in both formats the host
  sends: its `vt` first paint and libghostty-vt's binary snapshot. They are
  cached in `.cache/` until the corpus changes.
- `budgets.mjs` checks the results against the budgets.
- `corpus/` holds the recorded sessions and the generator of the synthetic
  workloads. Its README describes each item.

The input, selection and accessibility checks, and the hand checks, are in
`checks.md`.

A renderer tells the probes when it draws through the `BenchHooks` of
`ui/src/components/terminal/view.ts`. Both renderers implement them.

## Cases

Each case runs in a new page with one renderer. The page talks to the fake
core over WebSockets, as the window talks to Pinrail's core.

| Case                | What it measures                                                                                                                                                                                                                                                             | Budget                                                          |
| ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| `load`              | The code and WebAssembly fetched and compiled, a view mounted, its first frame, and the bytes loaded.                                                                                                                                                                        | None                                                            |
| `burst`             | An 8 MB item as fast as credit allows (tui, styled, plain, alt): the time until drawn, the throughput and the frame gaps.                                                                                                                                                    | tui drawn within 500 ms; no frame over 50 ms for tui and styled |
| `stream`            | tui output paced at 2 MB/s for 5 s: frame time p50, p95 and max, and frames over 16, 33 and 50 ms.                                                                                                                                                                           | p95 ≤ 20 ms, max ≤ 50 ms                                        |
| `idle`              | Frames drawn per second when nothing changes, after a stream.                                                                                                                                                                                                                | 0                                                               |
| `input`             | Key events on the view's input element, encoded by the view, sent over the WebSocket to `cat` on a real PTY in raw mode, echoed back and drawn. With `load=1`, a second terminal streams 2 MB/s meanwhile.                                                                   | p50 / p95 / p99 ≤ 5 / 12 / 25 ms; with load, p95 ≤ 25 ms        |
| `attach`            | A new view restores the snapshot of 5,000 lines of history until its first frame, in the first of the view's formats. Two snapshots: agent output (tui and diff) and dense colour (styled).                                                                                  | ≤ 150 ms                                                        |
| `memory`            | Three views each holding 5,000 lines: the growth of the web content process and of WebAssembly memory, per view.                                                                                                                                                             | ≤ 30 MiB                                                        |
| `switch`            | A pooled view hidden while 256 KB of output arrive, then shown, until it is painted and current. Five times.                                                                                                                                                                 | ≤ 50 ms                                                         |
| `agents`            | Eight terminals stream tui output at 50 KB/s each for 10 s, with one shown and the others taken out of the page as the pool keeps them: draws per second of the shown and the hidden views, the main-thread time spent drawing, and the CPU used by the web content process. | None                                                            |
| `agents` `titles=1` | The same eight terminals, each only setting its title 10 times a second with a spinner before the text, as an agent waiting on a long tool call does: the draws per second of the shown view.                                                                                | None                                                            |
| `replay`            | Each recording at its recorded timing, with pauses shortened to 0.5 s, then as fast as possible.                                                                                                                                                                             | None                                                            |
| `screen`            | A screenshot of the width fixture, the links and a recording, to check by eye, in `results/screens/`.                                                                                                                                                                        | None                                                            |

## What the numbers mean

- Frame times are the gaps between animation frames. WebKit has no Long
  Tasks API, so a long task shows only as a long frame.
- Headless WebKit is not tied to a display and draws at a device pixel ratio
  of 1. The WKWebView runner draws at the screen's ratio, 2 on a Retina
  display, so its canvas has four times the pixels.
- WebKit rounds `performance.now()` to 1 ms, so the parts of the input
  latency under a millisecond show as 0 or 1.
- Process memory is the resident size of the web content process. GPU
  memory, which holds xterm.js's WebGL glyph atlas, is not counted.
- The budgets apply on the spec's reference machines: an Apple M1 with
  macOS 14, and a 4-core x86-64 machine with Ubuntu 24.04. Results from
  other machines compare the renderers with each other and with earlier
  runs on the same machine, not with the budgets.
- `mise run bench:host` measures the host: parse throughput, memory and
  idle CPU.
