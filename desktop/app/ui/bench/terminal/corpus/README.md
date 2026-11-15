# Terminal benchmark corpus

The input that the terminal benchmark replays (runtime-spec 8.6). All items
assume a terminal of 120 columns and 40 rows. The benchmark (`../run.mjs`)
plays them through its fake core.

## Synthetic workloads

`tools/gen.mjs` writes these into `synthetic/`. The output is deterministic,
so every machine benchmarks the same bytes. The benchmark writes them on its
first run, and git ignores them. Regenerate them with
`node corpus/tools/gen.mjs` (`--mb N` changes the burst size).

| Item           | Size    | What it exercises                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| -------------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `plain.bin`    | 8.0 MiB | ASCII lines of 60 to 110 characters: the parser's fast path and scrolling.                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `styled.bin`   | 8.0 MiB | Truecolor SGR on every word, bold and background colours, CJK, emoji, ZWJ and VS16 sequences, box drawing. Fills glyph atlases.                                                                                                                                                                                                                                                                                                                                                                                         |
| `tui.bin`      | 8.0 MiB | An agent TUI in the style of Claude Code. Each frame is wrapped in synchronized output (mode 2026), moves the cursor up over the live region, erases it and redraws a streaming Markdown reply, a spinner, the input box and the status line. Every eighth frame commits the reply or a tool call to the scrollback.                                                                                                                                                                                                    |
| `alt.bin`      | 8.0 MiB | A full-screen program on the alternate screen that repaints every cell with CUP and 256-colour SGR, inside synchronized output.                                                                                                                                                                                                                                                                                                                                                                                         |
| `diff.bin`     | 4.0 MiB | Large diffs, alternately as `git diff --color` prints them and as agents show edits, with line numbers and tinted full-width backgrounds (`CSI K`).                                                                                                                                                                                                                                                                                                                                                                     |
| `unicode.bin`  | 1.7 KiB | A width fixture. Each row prints a grapheme cluster in brackets and then a bar at the column where it belongs if the cluster has the width agents compute. A renderer that measures differently leaves a gap or overlaps the bar. It covers CJK, Hangul, kana, wide emoji, VS16 and VS15, ZWJ sequences, skin tones, flags, keycaps, combining marks, Hangul jamo, a zero-width space, box drawing, Powerline and Nerd Font glyphs, and Braille spinners. A Powerline prompt and a box follow, which must have no gaps. |
| `links.bin`    | 1.0 MiB | OSC 8 hyperlinks with ST and BEL terminators, link ids, `file://` links and long links that wrap, mixed with plain URLs.                                                                                                                                                                                                                                                                                                                                                                                                |
| `trickle.cast` | 25 KiB  | A quiet minute (asciicast v2): typing echoed one character at a time, spinners redrawn every 80 ms, and occasional lines. It exercises the idle and small-write paths.                                                                                                                                                                                                                                                                                                                                                  |

The 8 MiB items are the bursts of the budgets in spec 8.6. The benchmark also
paces them at a byte rate (2 MB/s by default) to imitate streaming agent
output.

## Recordings

Real sessions in asciicast v2, in `recordings/`. Each was recorded
unattended in a throwaway folder holding `tools/demo/app.py`, with the plan
in `tools/plans/`. Each was scrubbed by `tools/scrub.py` and then read in
full for secrets: the user name, the home folder, the host name, an email
address and session ids were replaced with placeholders of the same length,
and no token, key or other personal data remained.

| Item                     | Size    | Length | What it holds                                                                                                                                                                                                                                                                                                                       |
| ------------------------ | ------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `claude-code.cast`       | 131 KiB | 45 s   | Claude Code 2.1.322, permission mode `default`. A turn that reads a file (tool output), asks to run `python3 app.py` (a permission prompt, answered yes), asks to edit (a second prompt) and shows a 41-line diff of the rename. The window is then resized to 100×30 and back, `/clear` is run, and the session ends with `/exit`. |
| `claude-code-long.cast`  | 99 KiB  | 51 s   | Claude Code 2.1.322: a long streamed Markdown reply with a heading, a table, a fenced code block and a numbered list, including a permission prompt for a command that Claude chose to run.                                                                                                                                         |
| `codex-model-error.cast` | 36 KiB  | 34 s   | Codex CLI 0.163.0: the start-up screen and its animation, the prompt, an error from the API (the configured model is not available to this CLI version), a resize to 100×30 and back, and `/new`. No turn ran; see below.                                                                                                           |

### Recordings still to make

A full Codex turn is still missing, because Codex rejected the model it was
configured with when the recording above was made. To record one with a
model your Codex accepts, run:

```sh
corpus/tools/record-session.sh codex --plan corpus/tools/plans/codex.json -- \
  codex -m <model> "Read app.py and summarise it in three short bullet points. Then run python3 app.py and tell me the largest total. Then rename the function compute to calculate_total everywhere in app.py."
```

Recordings worth adding by hand, without a plan, so that you drive the
session: a long turn with a lot of tool output, Codex's approval prompts,
and an agent session in a terminal with a non-default font size.

```sh
corpus/tools/record-session.sh my-session -- claude
```

## Tools

- `tools/gen.mjs` generates the synthetic workloads.
- `tools/record.py` records a program in a PTY as asciicast v2, either
  driven by you or following a plan of steps (wait for text, answer
  prompts, send keys, resize). The child gets a small fixed environment,
  never the caller's tokens. Typed input is not recorded.
- `tools/scrub.py` replaces names, paths, the host name, emails and session
  ids with placeholders of the same length, then lists anything that still
  looks like a secret and exits with status 1 if it found any. It can miss
  a value that a program draws in pieces, with cursor moves between them,
  so read the recording's text in full before you keep it.
- `tools/record-session.sh` runs the three steps in a new throwaway folder.
- `tools/plans/*.json` are the plans the recordings above were made with.
