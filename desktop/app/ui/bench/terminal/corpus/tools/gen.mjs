#!/usr/bin/env node
// Generates the synthetic workloads of the terminal benchmark corpus.
//
//   node corpus/tools/gen.mjs [--out corpus/synthetic] [--mb 8]
//
// The output is deterministic: every run writes the same bytes. Workloads are
// written as raw byte streams (.bin) that the fake core replays at a chosen
// rate, except `trickle`, whose timing matters and which is an asciicast v2
// file. All workloads assume a 120x40 terminal.

import { mkdirSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const opt = (name, dflt) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : dflt;
};
const OUT = opt("out", join(here, "..", "synthetic"));
const MB = Number(opt("mb", 8));
const BURST = MB * 1024 * 1024;
// an SGR sequence, to take the colours out of a line
// eslint-disable-next-line no-control-regex
const SGR = /\x1b\[[0-9;]*m/g;
const COLS = 120;
const ROWS = 40;

mkdirSync(OUT, { recursive: true });

// mulberry32: small, fast and deterministic.
function rng(seed) {
  let a = seed >>> 0;
  const next = () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  const int = (lo, hi) => lo + Math.floor(next() * (hi - lo + 1));
  const pick = (xs) => xs[Math.floor(next() * xs.length)];
  return { next, int, pick };
}

const enc = new TextEncoder();
const ESC = "\x1b";
const CSI = `${ESC}[`;
const SYNC_ON = `${CSI}?2026h`;
const SYNC_OFF = `${CSI}?2026l`;
const fg = (r, g, b) => `${CSI}38;2;${r};${g};${b}m`;
const bg = (r, g, b) => `${CSI}48;2;${r};${g};${b}m`;
const RESET = `${CSI}0m`;

// Collects strings into one buffer of a fixed size, cutting at the limit.
class Sink {
  constructor(limit) {
    this.limit = limit;
    this.parts = [];
    this.size = 0;
  }
  write(s) {
    if (this.full) return;
    const b = enc.encode(s);
    this.parts.push(b);
    this.size += b.length;
  }
  get full() {
    return this.size >= this.limit;
  }
  bytes() {
    const out = new Uint8Array(this.size);
    let o = 0;
    for (const p of this.parts) {
      out.set(p, o);
      o += p.length;
    }
    return out;
  }
}

const WORDS = (
  "the a terminal renderer frame buffer parse cursor scroll line cell glyph font width wide " +
  "agent session review tool output token stream render paint commit change test build run " +
  "function value return error result module import export const let async await promise type " +
  "interface struct impl trait match option some none ok err self string vector map key index"
).split(" ");
const CJK = ["终端", "渲染器", "性能", "測試", "テスト", "端末", "描画", "한국어", "터미널", "文字列"];
const EMOJI = ["🚀", "✅", "🔥", "📦", "🧪", "🎉", "💡", "🐛", "⚡", "🔧"];
const ZWJ = ["👩‍💻", "👨‍👩‍👧‍👦", "🏳️‍🌈", "🧑🏽‍🔬", "👁️‍🗨️"];
const VS16 = ["❤️", "⚠️", "✔️", "☺️", "✏️"];
const BOX = ["─", "│", "┌", "┐", "└", "┘", "├", "┤", "┬", "┴", "┼", "═", "║", "╭", "╮", "╯", "╰"];

function sentence(r, n) {
  const w = [];
  for (let i = 0; i < n; i++) w.push(r.pick(WORDS));
  return w.join(" ");
}

// plain: ASCII lines of 60 to 110 characters.
function plain() {
  const r = rng(1);
  const s = new Sink(BURST);
  while (!s.full) {
    let line = "";
    const len = r.int(60, 110);
    while (line.length < len) line += r.pick(WORDS) + " ";
    s.write(line.slice(0, len) + "\r\n");
  }
  return s.bytes();
}

// styled: truecolor SGR on every word, with CJK, emoji, ZWJ, VS16 and box drawing.
function styled() {
  const r = rng(2);
  const s = new Sink(BURST);
  while (!s.full) {
    let line = "";
    let width = 0;
    while (width < 100) {
      const kind = r.next();
      let word;
      let w;
      if (kind < 0.7) {
        word = r.pick(WORDS);
        w = word.length;
      } else if (kind < 0.8) {
        word = r.pick(CJK);
        w = [...word].length * 2;
      } else if (kind < 0.86) {
        word = r.pick(EMOJI);
        w = 2;
      } else if (kind < 0.9) {
        word = r.pick(ZWJ);
        w = 2;
      } else if (kind < 0.93) {
        word = r.pick(VS16);
        w = 2;
      } else {
        word = r.pick(BOX).repeat(r.int(2, 6));
        w = [...word].length;
      }
      const bold = r.next() < 0.2 ? `${CSI}1m` : "";
      const back = r.next() < 0.15 ? bg(r.int(0, 80), r.int(0, 80), r.int(0, 80)) : "";
      line += `${fg(r.int(60, 255), r.int(60, 255), r.int(60, 255))}${bold}${back}${word}${RESET} `;
      width += w + 1;
    }
    s.write(line + "\r\n");
  }
  return s.bytes();
}

// Markdown-ish lines of an assistant message, already styled the way agent
// TUIs print them.
function markdownLine(r, inCode) {
  if (inCode) {
    const kw = `${fg(198, 120, 221)}${r.pick(["const", "let", "fn", "return", "if", "await"])}${RESET}`;
    const id = `${fg(97, 175, 239)}${r.pick(WORDS)}${RESET}`;
    const str = `${fg(152, 195, 121)}"${sentence(r, r.int(1, 3))}"${RESET}`;
    return `  ${" ".repeat(r.int(0, 3) * 2)}${kw} ${id} = ${str};`;
  }
  const k = r.next();
  if (k < 0.1) return `${CSI}1m## ${sentence(r, r.int(2, 5))}${RESET}`;
  if (k < 0.35) return `  - ${sentence(r, r.int(4, 10))} \`${fg(229, 192, 123)}${r.pick(WORDS)}${RESET}\``;
  return `${sentence(r, r.int(5, 9))} ${CSI}1m${r.pick(WORDS)}${RESET} ${sentence(r, r.int(3, 8))}`;
}

const SPIN = ["·", "✢", "✳", "✶", "✻", "✽"];

// tui: imitates Claude Code. Each frame is wrapped in synchronized output,
// moves up over the live region, erases it, prints any committed lines and
// redraws the live region: the streaming reply, a spinner, the input box and
// the status line.
function tui() {
  const r = rng(3);
  const s = new Sink(BURST);
  let live = 0; // lines in the live region drawn last frame
  let reply = []; // lines of the reply being streamed
  let inCode = false;
  let frame = 0;
  let tokens = 0;
  while (!s.full) {
    frame++;
    let out = SYNC_ON;
    if (live > 0) out += `\r${CSI}${live}A${CSI}J`;
    // Every eighth frame commits the reply (or a tool call) to the scrollback.
    if (frame % 8 === 0) {
      if (r.next() < 0.4) {
        out += `${fg(130, 200, 130)}⏺${RESET} ${CSI}1mBash${RESET}(${r.pick(["ls -la", "npm test", "cargo build", "git status"])})\r\n`;
        const n = r.int(4, 18);
        for (let i = 0; i < n; i++) out += `  ${i === 0 ? "⎿" : " "}  ${CSI}2m${sentence(r, r.int(3, 12))}${RESET}\r\n`;
      } else {
        out += `${fg(255, 255, 255)}⏺${RESET} `;
        for (const l of reply) out += l + "\r\n";
      }
      out += "\r\n";
      reply = [];
    }
    // Stream a few more words into the reply.
    const add = r.int(1, 3);
    for (let i = 0; i < add; i++) {
      if (r.next() < 0.08) {
        reply.push(`  ${fg(120, 120, 120)}\`\`\`${RESET}`);
        inCode = !inCode;
      } else reply.push(markdownLine(r, inCode));
    }
    if (reply.length > 14) reply = reply.slice(-14);
    tokens += r.int(5, 40);
    const lines = [...reply, ""];
    lines.push(
      `${fg(215, 119, 87)}${SPIN[frame % SPIN.length]} ${r.pick(["Thinking", "Reading", "Pondering", "Editing"])}…${RESET} ${CSI}2m(${Math.floor(frame / 10)}s · ↑ ${(tokens / 1000).toFixed(1)}k tokens · esc to interrupt)${RESET}`,
    );
    lines.push("");
    lines.push(`${fg(136, 136, 136)}╭${"─".repeat(COLS - 2)}╮${RESET}`);
    lines.push(`${fg(136, 136, 136)}│${RESET} > ${" ".repeat(COLS - 5)}${fg(136, 136, 136)}│${RESET}`);
    lines.push(`${fg(136, 136, 136)}╰${"─".repeat(COLS - 2)}╯${RESET}`);
    lines.push(`  ${CSI}2m⏵⏵ accept edits on (shift+tab to cycle)${RESET}`);
    out += lines.join("\r\n");
    live = lines.length - 1;
    out += SYNC_OFF;
    s.write(out);
  }
  return s.bytes();
}

// alt: a full-screen application on the alternate screen that repaints every
// cell with CUP and 256-colour SGR.
function alt() {
  const r = rng(4);
  const s = new Sink(BURST - 16);
  s.write(`${CSI}?1049h${CSI}?25l`);
  while (!s.full) {
    let out = SYNC_ON;
    for (let y = 1; y <= ROWS; y++) {
      out += `${CSI}${y};1H`;
      let x = 0;
      while (x < COLS) {
        const n = Math.min(COLS - x, r.int(4, 16));
        out += `${CSI}38;5;${r.int(16, 231)};48;5;${r.int(232, 255)}m${sentence(r, 3).padEnd(n).slice(0, n)}`;
        x += n;
      }
    }
    out += RESET + SYNC_OFF;
    s.write(out);
  }
  const b = s.bytes();
  const tail = enc.encode(`${CSI}?25h${CSI}?1049l`);
  const out = new Uint8Array(b.length + tail.length);
  out.set(b);
  out.set(tail, b.length);
  return out;
}

// diff: a large change shown two ways, as `git diff --color` prints it and as
// agents show edits, with line numbers and tinted backgrounds.
function diff() {
  const r = rng(5);
  const s = new Sink(Math.round(BURST / 2));
  let file = 0;
  while (!s.full) {
    file++;
    const name = `src/${r.pick(WORDS)}/${r.pick(WORDS)}_${file}.ts`;
    if (file % 2) {
      s.write(
        `${CSI}1mdiff --git a/${name} b/${name}${RESET}\r\n${CSI}1m--- a/${name}${RESET}\r\n${CSI}1m+++ b/${name}${RESET}\r\n`,
      );
      for (let h = 0; h < r.int(2, 6); h++) {
        const at = r.int(1, 2000);
        s.write(`${CSI}36m@@ -${at},12 +${at},14 @@${RESET} function ${r.pick(WORDS)}()\r\n`);
        for (let i = 0; i < r.int(10, 40); i++) {
          const k = r.next();
          const code = markdownLine(r, true).replace(SGR, "");
          if (k < 0.25) s.write(`${CSI}31m-${code}${RESET}\r\n`);
          else if (k < 0.55) s.write(`${CSI}32m+${code}${RESET}\r\n`);
          else s.write(` ${code}\r\n`);
        }
      }
    } else {
      s.write(`${fg(130, 200, 130)}⏺${RESET} ${CSI}1mUpdate${RESET}(${name})\r\n`);
      s.write(`  ⎿  Updated ${CSI}1m${name}${RESET} with ${r.int(1, 40)} additions and ${r.int(1, 40)} removals\r\n`);
      let n = r.int(1, 3000);
      for (let i = 0; i < r.int(20, 80); i++) {
        const k = r.next();
        const code = markdownLine(r, true)
          .replace(SGR, "")
          .slice(0, COLS - 14);
        const num = String(n++).padStart(6);
        if (k < 0.25) s.write(`${bg(61, 1, 0)}${num} - ${code}${CSI}K${RESET}\r\n`);
        else if (k < 0.55) s.write(`${bg(2, 40, 0)}${num} + ${code}${CSI}K${RESET}\r\n`);
        else s.write(`${CSI}2m${num}${RESET}   ${code}\r\n`);
      }
    }
  }
  return s.bytes();
}

// unicode: a correctness fixture. Each row prints a cluster between brackets,
// then moves to the column where the closing bar belongs if the cluster has
// the expected width (as agents' string-width computes it) and prints `|`. A
// renderer that measures the cluster differently draws the bar apart from `]`.
const UNICODE_ROWS = [
  ["ASCII", "abc", 3],
  ["CJK", "漢字", 4],
  ["Hangul", "한글", 4],
  ["Kana", "カタカナ", 8],
  ["Emoji (wide)", "🚀", 2],
  ["VS16 heart", "❤️", 2],
  ["VS16 warning", "⚠️", 2],
  ["VS16 check", "✔️", 2],
  ["VS15 heart (text)", "❤︎", 1],
  ["Bare U+2764", "❤", 1],
  ["ZWJ technologist", "👩‍💻", 2],
  ["ZWJ family", "👨‍👩‍👧‍👦", 2],
  ["ZWJ rainbow flag", "🏳️‍🌈", 2],
  ["Skin tone", "👍🏽", 2],
  ["Flag GR", "🇬🇷", 2],
  ["Keycap", "1️⃣", 2],
  ["Combining acute", "é", 1],
  ["Hangul jamo", "각", 2],
  ["Zero width space", "a​b", 2],
  ["Box drawing", "╭─┬─╮", 5],
  ["Powerline", "", 2],
  ["Nerd Font home", "", 1],
  ["Braille spinner", "⠋⠙⠹", 3],
];

function unicode() {
  let out = `${CSI}2J${CSI}H${CSI}1mWidth fixture: the bar must touch the closing bracket.${RESET}\r\n\r\n`;
  for (const [label, text, width] of UNICODE_ROWS) {
    // label in columns 1-24, cluster from column 26, bar at 26 + 1 + width + 1
    out += `${label.padEnd(24)} [${text}]${CSI}${26 + 1 + width + 1}G|  ${CSI}2m(${width})${RESET}\r\n`;
  }
  out += "\r\n";
  // A Powerline prompt and a box, which must have no gaps.
  out += `${bg(40, 90, 160)}${fg(255, 255, 255)} ~/project ${RESET}${fg(40, 90, 160)}${bg(60, 140, 60)}${fg(255, 255, 255)} main ${RESET}${fg(60, 140, 60)}${RESET}\r\n`;
  out += `╭${"─".repeat(30)}╮\r\n│${" ".repeat(30)}│\r\n╰${"─".repeat(30)}╯\r\n`;
  return enc.encode(out);
}

// links: OSC 8 hyperlinks with ST and BEL terminators, ids, long URLs and
// plain URLs, repeated to about 1 MB.
function links() {
  const r = rng(6);
  const s = new Sink(1024 * 1024);
  let n = 0;
  while (!s.full) {
    n++;
    const url = `https://example.com/${r.pick(WORDS)}/${n}?q=${r.pick(WORDS)}`;
    const k = n % 5;
    if (k === 0)
      s.write(`See ${ESC}]8;;${url}${ESC}\\${CSI}4m${r.pick(WORDS)} docs${RESET}${ESC}]8;;${ESC}\\ for details.\r\n`);
    else if (k === 1)
      s.write(`${ESC}]8;id=ref${n};${url}\x07[${n}] link with an id and a BEL terminator${ESC}]8;;\x07\r\n`);
    else if (k === 2) s.write(`Plain URL: ${url}\r\n`);
    else if (k === 3)
      s.write(
        `${ESC}]8;;file:///home/user/project/src/${r.pick(WORDS)}.ts${ESC}\\src/${r.pick(WORDS)}.ts:${r.int(1, 999)}${ESC}]8;;${ESC}\\\r\n`,
      );
    else
      s.write(
        `${ESC}]8;;https://example.com/${"long/".repeat(30)}${n}${ESC}\\a link that wraps ${sentence(r, 20)}${ESC}]8;;${ESC}\\\r\n`,
      );
  }
  return s.bytes();
}

// trickle: a quiet minute of small writes, as typing echo and a spinner
// produce, as asciicast v2.
function trickle() {
  const r = rng(7);
  const events = [];
  let t = 0;
  const emit = (s) => events.push([Number(t.toFixed(6)), "o", s]);
  emit("$ ");
  while (t < 60) {
    const k = r.next();
    if (k < 0.5) {
      // a typed word, one character at a time
      for (const ch of r.pick(WORDS) + " ") {
        t += r.int(40, 160) / 1000;
        emit(ch);
      }
    } else if (k < 0.85) {
      // a spinner for a second or two
      const n = r.int(10, 25);
      for (let i = 0; i < n; i++) {
        t += 0.08;
        emit(`\r${CSI}2K${"⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"[i % 10]} Working…`);
      }
      emit(`\r${CSI}2K`);
    } else {
      t += r.int(100, 600) / 1000;
      emit(`\r\n${sentence(r, r.int(4, 12))}\r\n$ `);
    }
  }
  const header = {
    version: 2,
    width: COLS,
    height: ROWS,
    timestamp: 1767225600,
    env: { TERM: "xterm-256color", SHELL: "/bin/bash" },
    title: "trickle (synthetic)",
  };
  return [JSON.stringify(header), ...events.map((e) => JSON.stringify(e))].join("\n") + "\n";
}

const outputs = [
  ["plain.bin", plain],
  ["styled.bin", styled],
  ["tui.bin", tui],
  ["alt.bin", alt],
  ["diff.bin", diff],
  ["unicode.bin", unicode],
  ["links.bin", links],
  ["trickle.cast", trickle],
];
for (const [name, fn] of outputs) {
  const t0 = Date.now();
  const data = fn();
  writeFileSync(join(OUT, name), data);
  const size = typeof data === "string" ? Buffer.byteLength(data) : data.length;
  console.log(`${name.padEnd(14)} ${(size / 1048576).toFixed(2).padStart(6)} MiB  ${Date.now() - t0} ms`);
}
