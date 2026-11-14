// The libghostty terminal's keyboard, with the real library and real key
// events in WebKit.
import { afterEach, expect, test } from "vitest";
import { userEvent } from "vitest/browser";
import { GhosttyCore } from "./core";
import { Keyboard, type KeyboardOptions } from "./keyboard";
import { GhosttyRuntime } from "./runtime";

const theme = { foreground: { r: 1, g: 1, b: 1 }, background: { r: 0, g: 0, b: 0 }, cursor: { r: 1, g: 1, b: 1 } };
const text = (bytes: Uint8Array) => new TextDecoder().decode(bytes);
let gone: (() => void)[] = [];
afterEach(() => {
  gone.forEach((g) => g());
  gone = [];
});

async function rig(options: Partial<KeyboardOptions> = {}) {
  const host = document.createElement("div");
  host.style.position = "relative";
  document.body.append(host);
  const core = GhosttyCore.create(await GhosttyRuntime.instantiate(), 80, 24, 8, 16, theme, () => {});
  const sent: [string, string][] = [];
  const copied: string[] = [];
  const keyboard = new Keyboard(host, core, (bytes, kind) => sent.push([kind, text(bytes)]), {
    mac: false,
    optionAsMeta: "off",
    newline: null,
    copy: (t) => copied.push(t),
    ...options,
  });
  keyboard.focus();
  gone.push(() => (keyboard.dispose(), core.dispose(), host.remove()));
  const write = (s: string) => core.write(new TextEncoder().encode(s));
  return { core, keyboard, sent, copied, write };
}

test("keys go to the program, encoded for its modes", async () => {
  const { sent, write } = await rig();
  await userEvent.keyboard("a{ArrowUp}");
  // the program asks for application cursor keys
  write("\x1b[?1h");
  await userEvent.keyboard("{ArrowUp}{Control>}c{/Control}");
  expect(sent).toEqual([
    ["key", "a"],
    ["key", "\x1b[A"],
    ["key", "\x1bOA"],
    ["key", "\x03"],
  ]);
});

test("Shift+Enter sends the agent's new line keys", async () => {
  const { sent } = await rig({ newline: new Uint8Array([0x0a]) });
  await userEvent.keyboard("{Shift>}{Enter}{/Shift}{Enter}");
  expect(sent).toEqual([
    ["key", "\n"],
    ["key", "\r"],
  ]);
});

test("Option sends the character it composes, or Esc and the key as Meta", async () => {
  const off = await rig({ mac: true, optionAsMeta: "off" });
  const press = (target: HTMLElement) =>
    target.dispatchEvent(new KeyboardEvent("keydown", { key: "∫", code: "KeyB", altKey: true, bubbles: true }));
  press(off.keyboard.input);
  expect(off.sent).toEqual([["key", "∫"]]);
  const meta = await rig({ mac: true, optionAsMeta: "left" });
  press(meta.keyboard.input);
  expect(meta.sent).toEqual([["key", "\x1bb"]]);
});

test("a paste is bracketed when the program asks for it", async () => {
  const { keyboard, sent, write } = await rig();
  const paste = (t: string) => {
    const data = new DataTransfer();
    data.setData("text/plain", t);
    keyboard.input.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
  };
  paste("ls -la");
  write("\x1b[?2004h");
  paste("rm\x1b[201~ -rf");
  expect(sent[0]).toEqual(["paste", "ls -la"]);
  // bracketed, with the end marker inside it stripped
  expect(sent[1][0]).toBe("paste");
  expect(sent[1][1].startsWith("\x1b[200~")).toBe(true);
  expect(sent[1][1].endsWith("\x1b[201~")).toBe(true);
  expect(sent[1][1].slice(6, -6)).not.toContain("\x1b[201~");
});

test("a composition sends nothing until it ends, then its text once", async () => {
  const { keyboard, sent } = await rig();
  const input = keyboard.input;
  input.dispatchEvent(new CompositionEvent("compositionstart", { data: "" }));
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Process", keyCode: 229, isComposing: true }));
  input.dispatchEvent(new CompositionEvent("compositionupdate", { data: "に" }));
  expect(sent).toEqual([]);
  input.dispatchEvent(new CompositionEvent("compositionend", { data: "日本" }));
  expect(sent).toEqual([["key", "日本"]]);
});

test("Ctrl+Shift+C copies the selection on Linux, and Cmd+C on macOS", async () => {
  const linux = await rig();
  linux.write("hello world");
  linux.core.setSelection({ x: 0, y: 0 }, { x: 4, y: 0 });
  await userEvent.keyboard("{Control>}{Shift>}C{/Shift}{/Control}");
  expect(linux.copied).toEqual(["hello"]);
  expect(linux.sent).toEqual([]);
  const mac = await rig({ mac: true });
  mac.write("hello world");
  mac.core.setSelection({ x: 6, y: 0 }, { x: 10, y: 0 });
  await userEvent.keyboard("{Meta>}c{/Meta}");
  expect(mac.copied).toEqual(["world"]);
  // and without a selection, Ctrl+C is the program's interrupt
  mac.core.clearSelection();
  await userEvent.keyboard("{Control>}c{/Control}");
  expect(mac.sent).toEqual([["key", "\x03"]]);
});

test("focus is reported to a program that asks for it", async () => {
  const { keyboard, sent, write } = await rig();
  keyboard.blur();
  expect(sent).toEqual([]);
  write("\x1b[?1004h");
  keyboard.focus();
  keyboard.blur();
  expect(sent).toEqual([
    ["focus", "\x1b[I"],
    ["focus", "\x1b[O"],
  ]);
});
