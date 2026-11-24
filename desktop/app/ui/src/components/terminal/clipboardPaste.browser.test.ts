// ⌘V in a terminal: copied files as their paths, text as it is, and an
// image alone as Ctrl+V for the agent to read from the clipboard.
import { expect, test } from "vitest";
import { pasteInto } from "./clipboardPaste";

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
