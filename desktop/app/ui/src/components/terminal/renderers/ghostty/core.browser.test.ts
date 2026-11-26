// libghostty-vt as the window runs it: the app's WebAssembly, one instance
// per terminal, its replies and bell through the trampolines.
import { expect, test } from "vitest";
import { GhosttyCore } from "./core";
import { GhosttyRuntime } from "./runtime";

const theme = {
  foreground: { r: 237, g: 237, b: 239 },
  background: { r: 24, g: 25, b: 27 },
  cursor: { r: 237, g: 237, b: 239 },
};
const bytes = (text: string) => new TextEncoder().encode(text);

async function terminal() {
  const runtime = await GhosttyRuntime.instantiate();
  const replies: string[] = [];
  const core = GhosttyCore.create(runtime, 80, 24, 8, 16, theme, (data) =>
    replies.push(new TextDecoder().decode(data)),
  );
  return { runtime, core, replies };
}

test("each terminal gets an instance of the library with a memory of its own", async () => {
  const a = await terminal();
  const b = await terminal();
  expect(a.runtime.version).toMatch(/^1\.0\./);
  expect(a.runtime.memory).not.toBe(b.runtime.memory);
  a.core.dispose();
  b.core.dispose();
});

test("it answers the program's queries through its PTY, never as input", async () => {
  const { core, replies } = await terminal();
  core.write(bytes("ab\x1b[6n"));
  expect(replies.join("")).toBe("\x1b[1;3R");
  core.dispose();
});

test("it reads the title and the working folder the program sets, and rings the bell", async () => {
  const { core } = await terminal();
  let bells = 0;
  core.onBell(() => bells++);
  core.write(bytes("\x1b]2;Fixing the login\x07\x1b]7;file://host/work/app\x07\x07"));
  expect(core.title()).toBe("Fixing the login");
  expect(core.pwd()).toBe("file://host/work/app");
  expect(bells).toBe(1);
  core.dispose();
});

test("it tells when the program holds its output", async () => {
  const { core } = await terminal();
  expect(core.isSynchronizedOutput()).toBe(false);
  core.write(bytes("\x1b[?2026h"));
  expect(core.isSynchronizedOutput()).toBe(true);
  core.write(bytes("\x1b[?2026l"));
  expect(core.isSynchronizedOutput()).toBe(false);
  core.dispose();
});

test("its replies and bell carry on after the screen is reset", async () => {
  const { core, replies } = await terminal();
  let bells = 0;
  core.onBell(() => bells++);
  core.resetAndWrite(bytes("restored"));
  core.write(bytes("\x07\x1b[6n"));
  expect(bells).toBe(1);
  expect(replies.join("")).toBe("\x1b[1;9R");
  core.dispose();
});

test("a program copies to the clipboard with OSC 52, and cannot read it", async () => {
  const { core, replies } = await terminal();
  const copied: string[] = [];
  core.onClipboardWrite((text) => {
    copied.push(text);
    return true;
  });
  // "copied text", base64
  core.write(bytes("\x1b]52;c;Y29waWVkIHRleHQ=\x07"));
  expect(copied).toEqual(["copied text"]);
  // a snapshot replayed after a reset copies nothing again, and the
  // program's next copy arrives
  core.resetAndWrite(bytes("\x1b]52;c;cmVwbGF5ZWQ=\x07"));
  expect(copied).toEqual(["copied text"]);
  core.write(bytes("\x1b]52;c;YWdhaW4=\x07"));
  expect(copied).toEqual(["copied text", "again"]);
  // a read is refused: no reply carries the clipboard
  core.write(bytes("\x1b]52;c;?\x07"));
  expect(replies.join("")).not.toContain("52;c;");
  core.dispose();
});

test("each cell has its own colours, from the palette, as RGB or the default", async () => {
  const { core } = await terminal();
  core.write(
    bytes(
      // palette colour 1 set to a known red, then: palette, RGB, default,
      // inverse, and an erase that leaves cells with only a background
      "\x1b]4;1;rgb:c8/10/20\x1b\\" +
        "\x1b[31mp\x1b[0m\x1b[38;2;1;2;3mr\x1b[0md\x1b[7;38;2;9;8;7mi\x1b[0m" +
        "\x1b[48;2;40;50;60m\x1b[K\x1b[0m",
    ),
  );
  const cells = core.snapshot().rowData[0]!.cells;
  expect(cells[0]).toMatchObject({ text: "p", foreground: { r: 200, g: 16, b: 32 }, background: theme.background });
  expect(cells[1]).toMatchObject({ text: "r", foreground: { r: 1, g: 2, b: 3 } });
  expect(cells[2]).toMatchObject({ text: "d", foreground: theme.foreground });
  expect(cells[3]).toMatchObject({ text: "i", foreground: theme.background, background: { r: 9, g: 8, b: 7 } });
  expect(cells[10]).toMatchObject({ text: "", foreground: theme.foreground, background: { r: 40, g: 50, b: 60 } });
  core.dispose();
});

test("writes of every size reach the screen whole, as the input buffer grows", async () => {
  const { core } = await terminal();
  const last = () => core.screenLines().filter(Boolean).at(-1);
  // a small write, then larger ones that grow the buffer, then one past
  // its limit, then small ones again
  for (const size of [10, 5_000, 70_000, 300_000, 20]) {
    const line = `${size}:`.padEnd(70, "x");
    const lines = Math.max(1, Math.floor(size / 72));
    core.write(bytes(Array.from({ length: lines }, () => line).join("\r\n") + "\r\n"));
    expect(last(), `after a write of about ${size} bytes`).toBe(line);
  }
  core.dispose();
});

test("a snapshot that cannot be decoded leaves a blank terminal that works", async () => {
  const { core, replies } = await terminal();
  core.write(bytes("before the restore"));
  expect(() => core.restoreSnapshot(bytes("not a snapshot"), 80, 24, 8, 16)).toThrow();
  expect(core.screenLines().join("").trim()).toBe("");
  core.write(bytes("after\x1b[6n"));
  expect(core.screenLines()[0]).toContain("after");
  // its replies still reach the program
  expect(replies.join("")).toContain("\x1b[1;6R");
  core.dispose();
});
