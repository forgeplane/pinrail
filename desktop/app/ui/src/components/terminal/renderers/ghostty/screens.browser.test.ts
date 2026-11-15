// Whole terminal screens drawn by the libghostty renderer, compared with
// the reference images in __screenshots__/, one set per platform, as
// WebKit draws text differently on each. The text is in a font from the
// test's own dependencies, so that the machine's fonts change nothing but
// fallback glyphs. After an intended change to how the terminal looks,
// write the images again with `npm run test:browser -- --update`, and
// look at each before committing it.
import "./ghostty.css";
import { expect, test } from "vitest";
import { page } from "vitest/browser";
import bold from "@fontsource/jetbrains-mono/files/jetbrains-mono-latin-700-normal.woff2?url";
import boldItalic from "@fontsource/jetbrains-mono/files/jetbrains-mono-latin-700-italic.woff2?url";
import italic from "@fontsource/jetbrains-mono/files/jetbrains-mono-latin-400-italic.woff2?url";
import regular from "@fontsource/jetbrains-mono/files/jetbrains-mono-latin-400-normal.woff2?url";
import claudeCode from "../../../../../bench/terminal/corpus/recordings/claude-code.cast?raw";
import { GhosttyCore } from "./core";
import { CanvasRenderer } from "./renderer";
import { GhosttyRuntime } from "./runtime";

const FAMILY = "Pinrail Test Mono";
const font = { family: `"${FAMILY}"`, size: 13, lineHeight: 1.2 };
const theme = {
  foreground: { r: 212, g: 212, b: 212 },
  background: { r: 30, g: 30, b: 30 },
  cursor: { r: 212, g: 212, b: 212 },
};

// wide enough for a terminal of 120 columns
await page.viewport(1100, 800);

await Promise.all(
  [
    [regular, "400", "normal"],
    [bold, "700", "normal"],
    [italic, "400", "italic"],
    [boldItalic, "700", "italic"],
  ].map(async ([url, weight, style]) => {
    const face = new FontFace(FAMILY, `url(${url})`, { weight, style });
    document.fonts.add(await face.load());
  }),
);

/** Draws `output` on a terminal of `cols` × `rows`, after `setup`, and
 *  returns the element that holds the canvas and the cursor. */
async function screen(
  cols: number,
  rows: number,
  output: string | ((core: GhosttyCore, cell: { width: number; height: number }) => void),
  setup?: (core: GhosttyCore, renderer: CanvasRenderer) => void,
) {
  const host = document.createElement("div");
  host.className = "ghostty-terminal";
  document.body.append(host);
  const renderer = new CanvasRenderer(host, font, { r: 38, g: 79, b: 120 });
  const cell = renderer.cell;
  const core = GhosttyCore.create(
    await GhosttyRuntime.instantiate(),
    cols,
    rows,
    cell.width,
    cell.height,
    theme,
    () => {},
  );
  if (typeof output === "string") core.write(new TextEncoder().encode(output));
  else output(core, cell);
  setup?.(core, renderer);
  // not focused: the cursor is a hollow block that does not blink
  renderer.draw(core.snapshot(), false);
  host.style.width = `${cols * cell.width}px`;
  host.style.height = `${rows * cell.height}px`;
  return {
    host,
    dispose: () => {
      core.dispose();
      renderer.dispose();
      host.remove();
    },
  };
}

const sgr = (codes: string, text: string) => `\x1b[${codes}m${text}\x1b[0m`;

test("colours and text styles", async () => {
  const lines = [
    [...Array(8).keys()].map((i) => sgr(`3${i}`, `fg${i}`) + " " + sgr(`9${i}`, `br${i}`)).join(" "),
    [...Array(8).keys()].map((i) => sgr(`4${i}`, ` bg${i} `)).join(""),
    [...Array(32).keys()].map((i) => sgr(`48;5;${232 + (i % 24)}`, " ")).join("") +
      [...Array(24).keys()].map((i) => sgr(`38;2;${i * 10};${200 - i * 8};120`, "█")).join(""),
    `${sgr("1", "bold")} ${sgr("3", "italic")} ${sgr("1;3", "bold italic")} ${sgr("2", "faint")} ${sgr("7", "inverse")}`,
    `${sgr("9", "strikethrough")} ${sgr("53", "overline")} ${sgr("8", "invisible")}|`,
    [1, 2, 3, 4, 5].map((n) => sgr(`4:${n}`, `underline${n}`)).join(" "),
    sgr("4:3;58:2::255:80:80", "curly red underline") + " " + sgr("4;58:5:33", "blue underline"),
    "plain text with punctuation: -> => != == {} [] () <> ;:,. 0123456789",
  ];
  const { host, dispose } = await screen(80, 10, lines.join("\r\n"));
  await expect.element(page.elementLocator(host)).toMatchScreenshot("styles");
  dispose();
});

test("box drawing, blocks, Braille and Powerline", async () => {
  const lines = [
    "┌──────┬──────┐ ╔══════╦══════╗ ╭──────╮",
    "│ cell │ cell │ ║ cell ║ cell ║ │ ╱  ╲ │",
    "├──────┼──────┤ ╠══════╬══════╣ │ ╲  ╱ │",
    "└──────┴──────┘ ╚══════╩══════╝ ╰──────╯",
    "█▉▊▋▌▍▎▏ ▀▄ ░▒▓ ▖▗▘▙▚▛▜▝▞▟ ⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ⣿⡇⢸",
    `${sgr("44;30", " main ")}${sgr("34;42", "")}${sgr("42;30", " src ")}${sgr("32", "")} ${sgr("45", " round ")}`,
  ];
  const { host, dispose } = await screen(48, 7, lines.join("\r\n"));
  await expect.element(page.elementLocator(host)).toMatchScreenshot("glyphs");
  dispose();
});

test("a selection, find matches and the cursor", async () => {
  const text = ["select these words, then find", "the word find on every row", "find, find and find", "$ "].join(
    "\r\n",
  );
  const { host, dispose } = await screen(40, 5, text, (core, renderer) => {
    core.setSelection({ x: 7, y: 0 }, { x: 17, y: 0 });
    renderer.setMatches(
      [
        { row: 1, start: 9, end: 13 },
        { row: 2, start: 0, end: 4 },
        { row: 2, start: 15, end: 19 },
      ],
      { r: 120, g: 90, b: 0 },
    );
  });
  await expect.element(page.elementLocator(host)).toMatchScreenshot("selection");
  dispose();
});

test("the last screen of a Claude Code session", async () => {
  const { host, dispose } = await screen(120, 40, (core, cell) => {
    for (const line of claudeCode.split("\n").slice(1)) {
      if (!line.trim()) continue;
      const [, type, data] = JSON.parse(line) as [number, string, string];
      if (type === "o") core.write(new TextEncoder().encode(data));
      else if (type === "r") {
        const [cols, rows] = data.split("x").map(Number);
        core.resize(cols!, rows!, cell.width, cell.height);
      }
    }
  });
  await expect.element(page.elementLocator(host)).toMatchScreenshot("claude-code");
  dispose();
});
