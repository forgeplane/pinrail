// The renderer as shipped against its reference mode, which draws every
// row of every snapshot in full: after each step of a session, the two
// canvases must hold the same pixels. Any faster way of drawing, such as
// skipping rows or joining glyphs, must keep this test passing.
import { expect, test } from "vitest";
import { GhosttyCore } from "./core";
import { CanvasRenderer } from "./renderer";
import { GhosttyRuntime } from "./runtime";

const font = { family: "Menlo, 'DejaVu Sans Mono', monospace", size: 13, lineHeight: 1.2 };
const theme = {
  foreground: { r: 212, g: 212, b: 212 },
  background: { r: 30, g: 30, b: 30 },
  cursor: { r: 212, g: 212, b: 212 },
};
const sgr = (codes: string, text: string) => `\x1b[${codes}m${text}\x1b[0m`;

/** What a step does: output to write, or something done to the core and
 *  the renderers before the draw. */
type Step = [name: string, action: string | ((core: GhosttyCore, renderers: CanvasRenderer[]) => void)];

const lines = (n: number, from = 0) =>
  [...Array(n).keys()]
    .map((i) => `${sgr(`3${(i + from) % 8}`, `line ${i + from}`)} of output, with ${sgr("1", "bold")} and plain text`)
    .join("\r\n");

const STEPS: Step[] = [
  ["prompt", "$ "],
  [
    "styles",
    [
      [...Array(8).keys()].map((i) => sgr(`3${i};4${7 - i}`, ` c${i} `)).join("") +
        sgr("38;5;208;48;5;24", " 256 ") +
        sgr("38;2;250;120;90", " rgb "),
      `${sgr("1", "bold")} ${sgr("3", "italic")} ${sgr("1;3", "both")} ${sgr("2", "faint")} ${sgr("7", "inverse")} ${sgr("8", "hidden")} ${sgr("9", "struck")} ${sgr("53", "over")}`,
      [1, 2, 3, 4, 5].map((n) => sgr(`4:${n}`, `u${n}`)).join(" ") + " " + sgr("4:3;58:2::255:80:80", "curly red"),
      "punctuation -> => != == {} [] () <> ;:,. www fi fl 0123456789",
    ].join("\r\n") + "\r\n",
  ],
  ["wide and combining", "日本語 한국어 中文 👍🏽 🇬🇷 é ñ ✔︎ ⏺ 🙂\r\n"],
  ["box drawing", "┌──┬──┐ ╭──╮ █▓▒░ ⠋⠙⠹ \r\n└──┴──┘ ╰──╯\r\n"],
  ["scrolls", lines(30)],
  ["scrolls again", "\r\n" + lines(7, 30)],
  [
    "redraws a region in place",
    "\x1b[?2026h\x1b[3A\x1b[J" + sgr("32", "⠙ Working…") + "\r\n" + sgr("2", "  ⎿ reading files") + "\r\n> \x1b[?2026l",
  ],
  ["selects", (core) => core.setSelection({ x: 2, y: 3 }, { x: 20, y: 5 })],
  ["clears the selection", (core) => core.clearSelection()],
  [
    "highlights matches",
    (_, renderers) =>
      renderers.forEach((r) =>
        r.setMatches(
          [
            { row: 1, start: 0, end: 4 },
            { row: 8, start: 5, end: 9 },
          ],
          { r: 120, g: 90, b: 0 },
        ),
      ),
  ],
  ["clears the matches", (_, renderers) => renderers.forEach((r) => r.setMatches([]))],
  [
    "sets a scroll region under a status line",
    "\x1b[2J\x1b[12;1H" + sgr("7", " status: working ") + "\x1b[1;11r\x1b[11;1H",
  ],
  ...[...Array(6).keys()].map((i): Step => [
    `scrolls one line in the region (${i + 1})`,
    `\r\n${sgr(`3${i % 8}`, `streamed line ${i}`)} with plain words after it`,
  ]),
  ["scrolls three lines at once", "\r\nthree\r\nlines\r\nat once"],
  ["resets the scroll region", "\x1b[r"],
  ["clears the screen", "\x1b[2J\x1b[H$ ls\r\n"],
  ["fills the history", lines(40, 100)],
  // the wheel moves the viewport into the history and back
  ...([-5, -9, 3, 11] as const).map((rows): Step => [`scrolls the view by ${rows} rows`, (core) => core.scroll(rows)]),
];

async function compare(ratio: number) {
  const real = Object.getOwnPropertyDescriptor(window, "devicePixelRatio");
  Object.defineProperty(window, "devicePixelRatio", { configurable: true, get: () => ratio });
  const hosts = [0, 1].map(() => {
    const host = document.createElement("div");
    host.style.position = "relative";
    document.body.append(host);
    return host;
  });
  const shipped = new CanvasRenderer(hosts[0]!, font, { r: 38, g: 79, b: 120 });
  const reference = new CanvasRenderer(hosts[1]!, font, { r: 38, g: 79, b: 120 }, true);
  const { width, height } = shipped.cell;
  const core = GhosttyCore.create(await GhosttyRuntime.instantiate(), 60, 12, width, height, theme, () => {});
  const pixels = (r: CanvasRenderer) =>
    r.canvas.getContext("2d")!.getImageData(0, 0, r.canvas.width, r.canvas.height).data;
  try {
    for (const [name, action] of STEPS) {
      if (typeof action === "string") core.write(new TextEncoder().encode(action));
      else action(core, [shipped, reference]);
      const snapshot = core.snapshot();
      shipped.draw(snapshot, true);
      reference.draw(snapshot, true);
      const [a, b] = [pixels(shipped), pixels(reference)];
      expect(a.length, name).toBe(b.length);
      let differ = 0;
      const where = new Map<string, number>();
      const w = shipped.canvas.width;
      for (let i = 0; i < a.length; i++)
        if (a[i] !== b[i]) {
          differ++;
          const px = i >> 2;
          const key = `row ${Math.floor(px / w / (height * ratio))} y ${Math.floor(px / w)} x ${Math.floor((px % w) / (width * ratio))}`;
          where.set(key, (where.get(key) ?? 0) + 1);
        }
      const first = JSON.stringify([...where].slice(0, 6));
      expect(differ, `channel values that differ after "${name}" at pixel ratio ${ratio}, first at ${first}`).toBe(0);
    }
  } finally {
    core.dispose();
    shipped.dispose();
    reference.dispose();
    hosts.forEach((h) => h.remove());
    if (real) Object.defineProperty(window, "devicePixelRatio", real);
    else delete (window as { devicePixelRatio?: number }).devicePixelRatio;
  }
}

test("each draw at pixel ratio 1 matches the reference's", () => compare(1));

test("each draw at pixel ratio 2 matches the reference's", () => compare(2));
