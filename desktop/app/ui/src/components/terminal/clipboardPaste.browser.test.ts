// ⌘V in a terminal: copied files as their paths, text as it is, and an
// image alone as Ctrl+V for the agent to read from the clipboard.
import { expect, test } from "vitest";
import { dropInto, pasteInto } from "./clipboardPaste";
import { referenceRenderer } from "./testing/reference";
import type { ViewOptions } from "./view";

/** A view that records what it is asked to paste and press. */
function recorder() {
  const sent: string[] = [];
  return {
    sent,
    view: {
      paste: (text: string) => sent.push(`paste ${text}`),
      pressKey: (key: KeyboardEventInit) => sent.push(`key ${key.ctrlKey ? "ctrl+" : ""}${key.key}`),
    },
  };
}

/** A paste of what `fill` puts on the clipboard. */
function paste(fill: (data: DataTransfer) => void): ClipboardEvent {
  const data = new DataTransfer();
  fill(data);
  return new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true });
}

const png = () => new File([new Uint8Array([137, 80, 78, 71])], "image.png", { type: "image/png" });
const settled = () => new Promise((done) => setTimeout(done, 0));

test("files copied in Finder paste their paths, quoted, in place of their names", async () => {
  const { sent, view } = recorder();
  const event = paste((data) => {
    data.setData("text/plain", "a b.png");
    data.items.add(png());
  });
  pasteInto(view, event, { native: true, files: async () => ["/Users/me/a b.png", "/Users/me/notes.md"] });
  await settled();
  expect(event.defaultPrevented).toBe(true);
  expect(sent).toEqual(["paste '/Users/me/a b.png' /Users/me/notes.md"]);
});

test("an image alone is left to the agent with Ctrl+V", async () => {
  const { sent, view } = recorder();
  const event = paste((data) => data.items.add(png()));
  pasteInto(view, event, { native: true, files: async () => [] });
  await settled();
  expect(event.defaultPrevented).toBe(true);
  expect(sent).toEqual(["key ctrl+v"]);
});

test("in the app, text with no files copied is pasted as text", async () => {
  const { sent, view } = recorder();
  const event = paste((data) => data.setData("text/plain", "hello"));
  pasteInto(view, event, { native: true, files: async () => [] });
  await settled();
  expect(event.defaultPrevented).toBe(true);
  expect(sent).toEqual(["paste hello"]);
});

test("outside the app, text alone is left to the renderer", async () => {
  const { sent, view } = recorder();
  const event = paste((data) => data.setData("text/plain", "hello"));
  pasteInto(view, event, { native: false });
  await settled();
  expect(event.defaultPrevented).toBe(false);
  expect(sent).toEqual([]);
});

test("files a Linux file manager copied paste their paths from the URI list", async () => {
  const { sent, view } = recorder();
  const event = paste((data) => {
    data.setData("text/uri-list", "file:///home/me/My%20Shot.png");
    data.items.add(png());
  });
  pasteInto(view, event, { native: true, files: async () => [] });
  await settled();
  expect(sent).toEqual(["paste '/home/me/My Shot.png'"]);
});

/** A drop of what `fill` puts in the drag. */
function drop(fill: (data: DataTransfer) => void): DragEvent {
  const data = new DataTransfer();
  fill(data);
  return new DragEvent("drop", { dataTransfer: data, bubbles: true, cancelable: true });
}

test("files dropped on the terminal paste their paths, quoted, as one paste, and focus it", async () => {
  const { sent, view } = recorder();
  let focused = false;
  const event = drop((data) => {
    data.items.add(png());
    data.items.add(new File(["x"], "notes.md"));
  });
  dropInto({ ...view, focus: () => (focused = true) }, event, {
    native: true,
    files: async () => ["/Users/me/My Shot.png", "/Users/me/notes.md"],
  });
  await settled();
  expect(event.defaultPrevented).toBe(true);
  expect(sent).toEqual(["paste '/Users/me/My Shot.png' /Users/me/notes.md"]);
  expect(focused).toBe(true);
});

test("files a Linux file manager dropped paste their paths from the URI list", async () => {
  const { sent, view } = recorder();
  const event = drop((data) => {
    data.setData("text/uri-list", "file:///home/me/a.txt\r\nfile:///home/me/b%20c.txt");
    data.items.add(new File(["x"], "a.txt"));
  });
  dropInto({ ...view, focus: () => {} }, event, { native: false });
  await settled();
  expect(sent).toEqual(["paste /home/me/a.txt '/home/me/b c.txt'"]);
});

test("a drop of text, not files, is left alone", async () => {
  const { sent, view } = recorder();
  const event = drop((data) => data.setData("text/plain", "words"));
  dropInto({ ...view, focus: () => {} }, event, { native: true, files: async () => ["/x"] });
  await settled();
  expect(event.defaultPrevented).toBe(false);
  expect(sent).toEqual([]);
});

test("two dropped paths reach a program in bracketed paste mode as one bracketed paste", async () => {
  const view = referenceRenderer.create();
  const host = document.createElement("div");
  document.body.append(host);
  await view.mount(host, { cols: 80, rows: 24, scrollback: 0 } as ViewOptions);
  const typed: { data: string; kind: string }[] = [];
  view.onInput((bytes, kind) => typed.push({ data: new TextDecoder().decode(bytes), kind }));
  await new Promise<void>((done) => view.write(new TextEncoder().encode("\x1b[?2004h"), done));

  dropInto(
    view,
    drop((data) => data.items.add(png())),
    {
      native: true,
      files: async () => ["/Users/me/My Shot.png", "/Users/me/notes.md"],
    },
  );
  await settled();
  expect(typed).toEqual([{ data: "\x1b[200~'/Users/me/My Shot.png' /Users/me/notes.md\x1b[201~", kind: "paste" }]);
  view.dispose();
  host.remove();
});
